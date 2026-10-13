import Foundation
import UIKit

/// Decoded result of opening a document via the `doc_open` automation tool.
struct OpenedDocument {
    let docId: UInt64
    let pageCount: UInt64
    let editable: Bool
}

/// Drives a single `MobileSession` and exposes simple view-friendly state for SwiftUI.
///
/// The Rust engine is confined to the app's Documents directory (`MobileSession.withRoot`),
/// so any file the user picks must first be copied in there before `doc_open` can see it.
@MainActor
final class PDFSessionViewModel: ObservableObject {
    @Published var pageCount: Int = 0
    @Published var currentPage: Int = 1
    @Published var pageImage: UIImage?
    @Published var errorMessage: String?
    @Published var isLoading = false

    /// Render resolution. 144 DPI keeps memory reasonable while staying sharp on Retina screens.
    private let renderDpi = 144

    private var session: MobileSession?
    private var openedDoc: OpenedDocument?

    private var documentsRoot: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    /// Opens the sample PDF bundled with the app (`Resources/Sample.pdf`), so a fresh install
    /// shows real rendered content instead of the empty "No Document" state.
    func openBundledSample() {
        guard let url = Bundle.main.url(forResource: "Sample", withExtension: "pdf") else {
            errorMessage = "Couldn't locate the bundled sample PDF."
            return
        }
        open(url: url)
    }

    /// Copies the picked file into the sandboxed Documents root, then opens it through the
    /// engine and renders the first page.
    func open(url: URL) {
        errorMessage = nil
        isLoading = true

        let didAccess = url.startAccessingSecurityScopedResource()
        defer {
            if didAccess { url.stopAccessingSecurityScopedResource() }
        }

        do {
            let destination = documentsRoot.appendingPathComponent(url.lastPathComponent)
            if FileManager.default.fileExists(atPath: destination.path) {
                try FileManager.default.removeItem(at: destination)
            }
            try FileManager.default.copyItem(at: url, to: destination)

            let session = try MobileSession.withRoot(root: documentsRoot.path)
            self.session = session

            let openArgs = ["path": destination.lastPathComponent]
            let openResult = try call(session, name: "doc_open", args: openArgs)
            let doc = try Self.decodeOpenedDocument(from: openResult)
            self.openedDoc = doc
            self.pageCount = Int(doc.pageCount)
            self.currentPage = 1

            try renderCurrentPage()
        } catch {
            errorMessage = "Couldn't open \"\(url.lastPathComponent)\": \(error.localizedDescription)"
        }

        isLoading = false
    }

    func goToPage(_ page: Int) {
        guard page >= 1, page <= pageCount else { return }
        currentPage = page
        do {
            try renderCurrentPage()
        } catch {
            errorMessage = "Couldn't render page \(page): \(error.localizedDescription)"
        }
    }

    private func renderCurrentPage() throws {
        guard let session, let doc = openedDoc else { return }
        let args: [String: Any] = [
            "doc": doc.docId,
            "page": currentPage,
            "dpi": renderDpi,
        ]
        let result = try call(session, name: "page_render", args: args)
        pageImage = try Self.decodePageImage(from: result)
    }

    private func call(_ session: MobileSession, name: String, args: [String: Any]) throws -> [MobileContent] {
        let data = try JSONSerialization.data(withJSONObject: args)
        let json = String(data: data, encoding: .utf8) ?? "{}"
        return try session.call(name: name, argsJson: json)
    }

    private static func decodeOpenedDocument(from content: [MobileContent]) throws -> OpenedDocument {
        for item in content {
            if case let .json(json) = item {
                guard
                    let data = json.data(using: .utf8),
                    let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                    let doc = obj["doc"] as? NSNumber,
                    let pages = obj["pages"] as? NSNumber,
                    let editable = obj["editable"] as? Bool
                else {
                    throw DecodingError.malformed("doc_open")
                }
                return OpenedDocument(
                    docId: doc.uint64Value,
                    pageCount: pages.uint64Value,
                    editable: editable
                )
            }
        }
        throw DecodingError.malformed("doc_open")
    }

    private static func decodePageImage(from content: [MobileContent]) throws -> UIImage {
        for item in content {
            if case let .png(data, _, _) = item {
                guard let image = UIImage(data: data) else {
                    throw DecodingError.malformed("page_render")
                }
                return image
            }
        }
        throw DecodingError.malformed("page_render")
    }

    private enum DecodingError: LocalizedError {
        case malformed(String)

        var errorDescription: String? {
            switch self {
            case let .malformed(tool):
                return "Unexpected response shape from \(tool)"
            }
        }
    }
}
