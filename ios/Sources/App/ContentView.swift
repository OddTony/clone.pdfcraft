import SwiftUI

/// MVP viewer: pick a PDF, render pages through the native aRTI-PDF engine, and page through them.
struct ContentView: View {
    @StateObject private var viewModel = PDFSessionViewModel()
    @State private var showingPicker = false

    var body: some View {
        NavigationStack {
            VStack {
                if viewModel.isLoading {
                    ProgressView("Opening…")
                } else if let image = viewModel.pageImage {
                    ScrollView([.horizontal, .vertical]) {
                        Image(uiImage: image)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                    }
                    pageControls
                } else if let message = viewModel.errorMessage {
                    ContentUnavailableView("Couldn't Open PDF", systemImage: "exclamationmark.triangle", description: Text(message))
                } else {
                    ContentUnavailableView("No Document", systemImage: "doc.text", description: Text("Open a PDF to get started."))
                }
            }
            .navigationTitle("aRTI-PDF")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        showingPicker = true
                    } label: {
                        Label("Open", systemImage: "folder")
                    }
                }
            }
            .sheet(isPresented: $showingPicker) {
                DocumentPicker { url in
                    showingPicker = false
                    viewModel.open(url: url)
                }
            }
        }
    }

    private var pageControls: some View {
        HStack {
            Button {
                viewModel.goToPage(viewModel.currentPage - 1)
            } label: {
                Image(systemName: "chevron.left")
            }
            .disabled(viewModel.currentPage <= 1)

            Spacer()

            Text("Page \(viewModel.currentPage) of \(viewModel.pageCount)")
                .font(.footnote)
                .foregroundStyle(.secondary)

            Spacer()

            Button {
                viewModel.goToPage(viewModel.currentPage + 1)
            } label: {
                Image(systemName: "chevron.right")
            }
            .disabled(viewModel.currentPage >= viewModel.pageCount)
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
    }
}

#Preview {
    ContentView()
}
