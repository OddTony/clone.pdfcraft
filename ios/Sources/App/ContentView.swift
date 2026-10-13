import SwiftUI

/// MVP viewer: pick a PDF, render pages through the native aRTI-PDF engine, and page through them.
struct ContentView: View {
    @StateObject private var viewModel = PDFSessionViewModel()
    @State private var showingPicker = false

    var body: some View {
        NavigationStack {
            mainContent
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
                .onAppear {
                    guard viewModel.pageImage == nil, !viewModel.isLoading else { return }
                    viewModel.openBundledSample()
                }
        }
    }

    @ViewBuilder
    private var mainContent: some View {
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
                placeholder(title: "Couldn't Open PDF", systemImage: "exclamationmark.triangle", description: message)
                retryButton
            } else {
                placeholder(title: "No Document", systemImage: "doc.text", description: "Open a PDF to get started.")
                retryButton
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

    private var retryButton: some View {
        Button {
            viewModel.openBundledSample()
        } label: {
            Label("Try Sample", systemImage: "doc.text.fill")
        }
        .buttonStyle(.bordered)
        .padding(.top, 4)
    }

    /// iOS 16-compatible stand-in for `ContentUnavailableView` (iOS 17+).
    private func placeholder(title: String, systemImage: String, description: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.title2)
                .fontWeight(.semibold)
            Text(description)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#Preview {
    ContentView()
}
