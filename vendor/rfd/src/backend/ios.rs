//! PdfCraft patch: UIKit backend for iOS and iPadOS.
//!
//! UIKit has no open or save panels; this maps rfd's dialogs onto what it does have:
//!
//! - **Pick file(s)**: `UIDocumentPickerViewController` in copy mode. The copies the picker
//!   leaves in the app's temporary inbox are moved into the app's `Documents` folder, so they
//!   outlive the inbox (which iOS empties) and show in the Files app under the app's name. The
//!   returned paths are plain files the app may read and overwrite.
//! - **Pick folder(s)**: the picker in open mode for folders. Security-scoped access to the chosen
//!   folder is started and kept for the life of the process, so the app can create, rename and
//!   replace files inside it (atomic saves write a sibling and rename it over the target).
//! - **Save file**: an alert asking for the file name (prefilled with `set_file_name`), then the
//!   folder picker, starting in `Documents`. The result is the folder joined with the name.
//! - **Message dialogs**: `UIAlertController` with one action per button.
//!
//! Dialogs are presented from the top-most view controller of the key window. A dialog
//! requested off the main thread is dispatched to it. The synchronous API cannot wait on the
//! main thread (UIKit only delivers the answer to a running main run loop), so there it logs a
//! warning and returns as if cancelled; off the main thread it blocks until the answer arrives.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::{Retained, Weak};
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_ui_kit::{
    UIAlertAction, UIAlertActionStyle, UIAlertController, UIAlertControllerStyle, UIApplication,
    UIDocumentPickerDelegate, UIDocumentPickerViewController, UITextField, UIViewController,
    UIWindowScene,
};
use objc2_uniform_type_identifiers::{UTType, UTTypeFolder, UTTypeItem};

use super::ios_names::{free_name, save_name};
use crate::backend::{
    AsyncFilePickerDialogImpl, AsyncFileSaveDialogImpl, AsyncFolderPickerDialogImpl,
    AsyncMessageDialogImpl, DialogFutureType, FilePickerDialogImpl, FileSaveDialogImpl,
    FolderPickerDialogImpl, MessageDialogImpl,
};
use crate::message_dialog::{MessageButtons, MessageDialog, MessageDialogResult};
use crate::oneshot;
use crate::{FileDialog, FileHandle};

/// What the document picker is choosing.
#[derive(Clone, Debug)]
enum Mode {
    /// Existing files, copied into `Documents`.
    Files { multiple: bool },
    /// Existing folders, opened in place.
    Folders { multiple: bool },
    /// A folder to save `name` into.
    SaveInto { name: String },
}

/// The parts of a [`FileDialog`] the picker uses. Unlike the dialog (which may hold a raw window
/// handle) it can be sent to the main thread.
#[derive(Clone, Debug)]
struct Request {
    extensions: Vec<String>,
    start: Option<PathBuf>,
    file_name: Option<String>,
    title: Option<String>,
}

impl From<FileDialog> for Request {
    fn from(d: FileDialog) -> Self {
        Request {
            extensions: d.filters.into_iter().flat_map(|f| f.extensions).collect(),
            start: d.starting_directory,
            file_name: d.file_name,
            title: d.title,
        }
    }
}

type Paths = Option<Vec<PathBuf>>;

//
// Main thread
//

/// Run `f` on the main thread: now when already there, otherwise as soon as the main queue
/// gets to it. If it never can, `f` (and any sender it owns) is dropped, which reads as cancel.
fn on_main<F: FnOnce(MainThreadMarker) + Send + 'static>(f: F) {
    match MainThreadMarker::new() {
        Some(mtm) => f(mtm),
        None => DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                f(mtm);
            }
        }),
    }
}

/// The view controller to present from: the top of the key window's presentation stack.
fn presenter(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let app = UIApplication::sharedApplication(mtm);
    let mut window = None;
    for scene in app.connectedScenes().iter() {
        if let Ok(scene) = scene.downcast::<UIWindowScene>() {
            window = scene.keyWindow().or_else(|| scene.windows().firstObject());
            if window.is_some() {
                break;
            }
        }
    }
    // An app on the app-delegate lifecycle (winit 0.30) whose window has no scene: the app's own
    // window list, deprecated for scene-based apps but still filled for these.
    #[allow(deprecated)]
    let window = window
        .or_else(|| app.keyWindow())
        .or_else(|| app.windows().firstObject());
    let mut top = window?.rootViewController()?;
    while let Some(next) = top.presentedViewController() {
        top = next;
    }
    Some(top)
}

/// Present `vc`, or report that there is nothing to present it from.
fn present(mtm: MainThreadMarker, vc: &UIViewController) -> bool {
    match presenter(mtm) {
        Some(top) => {
            top.presentViewController_animated_completion(vc, true, None);
            true
        }
        None => {
            log::warn!("rfd: no window to present the dialog from");
            false
        }
    }
}

//
// Document picker
//

struct PickerIvars {
    mode: Mode,
    tx: RefCell<Option<oneshot::Sender<Paths>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `PickerDelegate` does not implement
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PdfCraftRfdDocumentPickerDelegate"]
    #[ivars = PickerIvars]
    struct PickerDelegate;

    unsafe impl NSObjectProtocol for PickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for PickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick(&self, _picker: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
            let urls: Vec<Retained<NSURL>> = urls.iter().collect();
            self.finish(Some(chosen(&self.ivars().mode, &urls)));
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn cancelled(&self, _picker: &UIDocumentPickerViewController) {
            self.finish(None);
        }
    }
);

thread_local! {
    /// UIKit holds its delegate weakly; these are kept alive here until they answer.
    static LIVE: RefCell<Vec<Retained<PickerDelegate>>> = const { RefCell::new(Vec::new()) };
}

impl PickerDelegate {
    fn new(mtm: MainThreadMarker, mode: Mode, tx: oneshot::Sender<Paths>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PickerIvars {
            mode,
            tx: RefCell::new(Some(tx)),
        });
        // SAFETY: `init` is NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn finish(&self, paths: Paths) {
        if let Some(tx) = self.ivars().tx.borrow_mut().take() {
            let _ = tx.send(paths.filter(|p| !p.is_empty()));
        }
        let me: *const Self = self;
        LIVE.with(|live| {
            live.borrow_mut()
                .retain(|d| !std::ptr::eq(Retained::as_ptr(d), me))
        });
    }
}

/// Turn the picked URLs into the paths the app gets back (see the module docs).
fn chosen(mode: &Mode, urls: &[Retained<NSURL>]) -> Vec<PathBuf> {
    let path = |url: &NSURL| url.path().map(|p| PathBuf::from(p.to_string()));
    match mode {
        Mode::Files { .. } => urls
            .iter()
            .filter_map(|u| path(u))
            .map(|p| keep_copy(&p))
            .collect(),
        Mode::Folders { .. } => urls
            .iter()
            .filter_map(|u| {
                keep_access(u);
                path(u)
            })
            .collect(),
        Mode::SaveInto { name } => urls
            .first()
            .and_then(|u| {
                keep_access(u);
                path(u)
            })
            .map(|dir| dir.join(name))
            .into_iter()
            .collect(),
    }
}

/// Start security-scoped access to `url` and never stop it: the app may write there at any
/// later time, as it could with a path from a desktop save panel.
fn keep_access(url: &NSURL) {
    // SAFETY: plain message send to a valid NSURL; balancing it with
    // `stopAccessingSecurityScopedResource` is optional (access ends with the process).
    if !unsafe { url.startAccessingSecurityScopedResource() } {
        log::warn!(
            "rfd: no security-scoped access to {:?}",
            url.path().map(|p| p.to_string())
        );
    }
}

/// The app's `Documents` folder (inside its container; `HOME` is the container on iOS).
fn documents() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents"))
}

/// Move a picked copy out of the temporary inbox into `Documents`, under a name that does not
/// replace an existing file. Falls back to the inbox copy when that fails.
fn keep_copy(copy: &Path) -> PathBuf {
    let Some(docs) = documents() else {
        return copy.to_path_buf();
    };
    if copy.starts_with(&docs) || std::fs::create_dir_all(&docs).is_err() {
        return copy.to_path_buf();
    }
    let Some(target) = free_name(&docs, copy) else {
        return copy.to_path_buf();
    };
    match std::fs::rename(copy, &target).or_else(|_| std::fs::copy(copy, &target).map(drop)) {
        Ok(()) => target,
        Err(e) => {
            log::warn!("rfd: could not keep {copy:?} in Documents: {e}");
            copy.to_path_buf()
        }
    }
}

/// The content types for the picker's filters; any item when there are none.
fn content_types(mode: &Mode, extensions: &[String]) -> Retained<NSArray<UTType>> {
    if !matches!(mode, Mode::Files { .. }) {
        // SAFETY: an immutable framework constant.
        return NSArray::from_slice(&[unsafe { UTTypeFolder }]);
    }
    let types: Vec<Retained<UTType>> = extensions
        .iter()
        .filter(|e| !e.is_empty() && *e != "*")
        .filter_map(|e| UTType::typeWithFilenameExtension(&NSString::from_str(e)))
        .collect();
    if types.is_empty() {
        // SAFETY: an immutable framework constant.
        NSArray::from_slice(&[unsafe { UTTypeItem }])
    } else {
        NSArray::from_retained_slice(&types)
    }
}

/// Show the document picker for `mode`; the answer goes to `tx`.
fn show_picker(mtm: MainThreadMarker, mode: Mode, req: &Request, tx: oneshot::Sender<Paths>) {
    let types = content_types(&mode, &req.extensions);
    let as_copy = matches!(mode, Mode::Files { .. });
    let picker = UIDocumentPickerViewController::initForOpeningContentTypes_asCopy(
        UIDocumentPickerViewController::alloc(mtm),
        &types,
        as_copy,
    );
    let multiple = match mode {
        Mode::Files { multiple } | Mode::Folders { multiple } => multiple,
        Mode::SaveInto { .. } => false,
    };
    picker.setAllowsMultipleSelection(multiple);
    picker.setShouldShowFileExtensions(true);
    let start = req.start.clone().or_else(|| {
        matches!(mode, Mode::SaveInto { .. })
            .then(documents)
            .flatten()
    });
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
        picker.setDirectoryURL(Some(&url));
    }
    let delegate = PickerDelegate::new(mtm, mode, tx);
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    LIVE.with(|live| live.borrow_mut().push(delegate.clone()));
    if !present(mtm, &picker) {
        delegate.finish(None);
    }
}

/// Ask for the file name, then show the folder picker.
fn show_save(mtm: MainThreadMarker, req: Request, tx: oneshot::Sender<Paths>) {
    let title = req.title.clone().unwrap_or_else(|| "Save As".to_owned());
    let alert = UIAlertController::alertControllerWithTitle_message_preferredStyle(
        Some(&NSString::from_str(&title)),
        Some(&NSString::from_str("Name the file, then choose a folder.")),
        UIAlertControllerStyle::Alert,
        mtm,
    );
    let initial = req.file_name.clone().unwrap_or_default();
    let configure = RcBlock::new(move |field: NonNull<UITextField>| {
        // SAFETY: UIKit passes the text field it just created, valid for this call.
        let field = unsafe { field.as_ref() };
        field.setText(Some(&NSString::from_str(&initial)));
    });
    alert.addTextFieldWithConfigurationHandler(Some(&configure));

    let tx = Rc::new(RefCell::new(Some(tx)));
    let weak_alert = Weak::from_retained(&alert);
    let cancel_tx = tx.clone();
    let cancel = RcBlock::new(move |_: NonNull<UIAlertAction>| {
        if let Some(tx) = cancel_tx.borrow_mut().take() {
            let _ = tx.send(None);
        }
    });
    let save = RcBlock::new(move |_: NonNull<UIAlertAction>| {
        let Some(tx) = tx.borrow_mut().take() else {
            return;
        };
        let typed = weak_alert
            .load()
            .and_then(|a| a.textFields())
            .and_then(|f| f.firstObject())
            .and_then(|f| f.text())
            .map(|t| t.to_string())
            .unwrap_or_default();
        let Some(name) = save_name(&typed, &req.extensions) else {
            let _ = tx.send(None);
            return;
        };
        let req = req.clone();
        // Present the picker once the alert has finished going away.
        DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                show_picker(mtm, Mode::SaveInto { name }, &req, tx);
            }
        });
    });
    alert.addAction(&UIAlertAction::actionWithTitle_style_handler(
        Some(&NSString::from_str("Cancel")),
        UIAlertActionStyle::Cancel,
        Some(&cancel),
        mtm,
    ));
    let save_action = UIAlertAction::actionWithTitle_style_handler(
        Some(&NSString::from_str("Choose Folder")),
        UIAlertActionStyle::Default,
        Some(&save),
        mtm,
    );
    alert.addAction(&save_action);
    alert.setPreferredAction(Some(&save_action));
    // If it can't be shown, the actions (and the sender) are dropped with it: cancel.
    present(mtm, &alert);
}

/// Start a pick on the main thread and return its answer as a future.
fn pick_async(dialog: FileDialog, mode: Mode) -> DialogFutureType<Paths> {
    let req = Request::from(dialog);
    let (tx, rx) = oneshot::channel();
    on_main(move |mtm| match mode {
        Mode::SaveInto { .. } => show_save(mtm, req, tx),
        mode => show_picker(mtm, mode, &req, tx),
    });
    Box::pin(async move { rx.await.ok().flatten() })
}

fn handles(paths: Paths) -> Option<Vec<FileHandle>> {
    paths.map(|p| p.into_iter().map(FileHandle::wrap).collect())
}

fn first(paths: Paths) -> Option<PathBuf> {
    paths.and_then(|p| p.into_iter().next())
}

//
// Waiting for an answer (synchronous API)
//

struct Unpark(std::thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Wait for `fut` on this thread, or give `fallback` on the main thread (see the module docs).
fn wait<T>(what: &str, fallback: T, fut: impl FnOnce() -> DialogFutureType<T>) -> T {
    if MainThreadMarker::new().is_some() {
        log::warn!("rfd: a blocking {what} can't wait on the iOS main thread; use the async API");
        return fallback;
    }
    let mut fut = fut();
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

//
// rfd's traits
//

impl FilePickerDialogImpl for FileDialog {
    fn pick_file(self) -> Option<PathBuf> {
        first(wait("file picker", None, || {
            pick_async(self, Mode::Files { multiple: false })
        }))
    }

    fn pick_files(self) -> Option<Vec<PathBuf>> {
        wait("file picker", None, || {
            pick_async(self, Mode::Files { multiple: true })
        })
    }
}

impl AsyncFilePickerDialogImpl for FileDialog {
    fn pick_file_async(self) -> DialogFutureType<Option<FileHandle>> {
        let fut = pick_async(self, Mode::Files { multiple: false });
        Box::pin(async move { first(fut.await).map(FileHandle::wrap) })
    }

    fn pick_files_async(self) -> DialogFutureType<Option<Vec<FileHandle>>> {
        let fut = pick_async(self, Mode::Files { multiple: true });
        Box::pin(async move { handles(fut.await) })
    }
}

impl FolderPickerDialogImpl for FileDialog {
    fn pick_folder(self) -> Option<PathBuf> {
        first(wait("folder picker", None, || {
            pick_async(self, Mode::Folders { multiple: false })
        }))
    }

    fn pick_folders(self) -> Option<Vec<PathBuf>> {
        wait("folder picker", None, || {
            pick_async(self, Mode::Folders { multiple: true })
        })
    }
}

impl AsyncFolderPickerDialogImpl for FileDialog {
    fn pick_folder_async(self) -> DialogFutureType<Option<FileHandle>> {
        let fut = pick_async(self, Mode::Folders { multiple: false });
        Box::pin(async move { first(fut.await).map(FileHandle::wrap) })
    }

    fn pick_folders_async(self) -> DialogFutureType<Option<Vec<FileHandle>>> {
        let fut = pick_async(self, Mode::Folders { multiple: true });
        Box::pin(async move { handles(fut.await) })
    }
}

impl FileSaveDialogImpl for FileDialog {
    fn save_file(self) -> Option<PathBuf> {
        first(wait("save dialog", None, || {
            pick_async(
                self,
                Mode::SaveInto {
                    name: String::new(),
                },
            )
        }))
    }
}

impl AsyncFileSaveDialogImpl for FileDialog {
    fn save_file_async(self) -> DialogFutureType<Option<FileHandle>> {
        let fut = pick_async(
            self,
            Mode::SaveInto {
                name: String::new(),
            },
        );
        Box::pin(async move { first(fut.await).map(FileHandle::wrap) })
    }
}

//
// Message dialogs
//

/// The alert's buttons, first to last, with what each answers and whether it is the cancel one.
fn buttons(buttons: &MessageButtons) -> Vec<(String, MessageDialogResult, bool)> {
    use MessageDialogResult as R;
    let custom = |s: &String| R::Custom(s.clone());
    match buttons {
        MessageButtons::Ok => vec![("OK".into(), R::Ok, false)],
        MessageButtons::OkCancel => vec![
            ("Cancel".into(), R::Cancel, true),
            ("OK".into(), R::Ok, false),
        ],
        MessageButtons::YesNo => vec![("No".into(), R::No, false), ("Yes".into(), R::Yes, false)],
        MessageButtons::YesNoCancel => vec![
            ("Cancel".into(), R::Cancel, true),
            ("No".into(), R::No, false),
            ("Yes".into(), R::Yes, false),
        ],
        MessageButtons::OkCustom(ok) => vec![(ok.clone(), custom(ok), false)],
        MessageButtons::OkCancelCustom(ok, cancel) => {
            vec![
                (cancel.clone(), custom(cancel), true),
                (ok.clone(), custom(ok), false),
            ]
        }
        MessageButtons::YesNoCancelCustom(yes, no, cancel) => vec![
            (cancel.clone(), custom(cancel), true),
            (no.clone(), custom(no), false),
            (yes.clone(), custom(yes), false),
        ],
    }
}

fn show_alert(
    mtm: MainThreadMarker,
    title: String,
    text: String,
    buttons: Vec<(String, MessageDialogResult, bool)>,
    tx: oneshot::Sender<MessageDialogResult>,
) {
    let title = (!title.is_empty()).then(|| NSString::from_str(&title));
    let text = (!text.is_empty()).then(|| NSString::from_str(&text));
    let alert = UIAlertController::alertControllerWithTitle_message_preferredStyle(
        title.as_deref(),
        text.as_deref(),
        UIAlertControllerStyle::Alert,
        mtm,
    );
    let tx = Rc::new(RefCell::new(Some(tx)));
    let mut preferred = None;
    for (label, result, is_cancel) in buttons {
        let tx = tx.clone();
        let handler = RcBlock::new(move |_: NonNull<UIAlertAction>| {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(result.clone());
            }
        });
        let style = if is_cancel {
            UIAlertActionStyle::Cancel
        } else {
            UIAlertActionStyle::Default
        };
        let action = UIAlertAction::actionWithTitle_style_handler(
            Some(&NSString::from_str(&label)),
            style,
            Some(&handler),
            mtm,
        );
        alert.addAction(&action);
        if !is_cancel {
            preferred = Some(action);
        }
    }
    alert.setPreferredAction(preferred.as_deref());
    present(mtm, &alert);
}

impl AsyncMessageDialogImpl for MessageDialog {
    fn show_async(self) -> DialogFutureType<MessageDialogResult> {
        let (title, text, buttons) = (self.title, self.description, buttons(&self.buttons));
        let (tx, rx) = oneshot::channel();
        on_main(move |mtm| show_alert(mtm, title, text, buttons, tx));
        Box::pin(async move { rx.await.unwrap_or_default() })
    }
}

impl MessageDialogImpl for MessageDialog {
    fn show(self) -> MessageDialogResult {
        wait("message dialog", MessageDialogResult::Cancel, || {
            self.show_async()
        })
    }
}
