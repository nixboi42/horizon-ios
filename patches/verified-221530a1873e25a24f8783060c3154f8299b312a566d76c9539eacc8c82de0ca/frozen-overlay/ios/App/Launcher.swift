import SwiftUI
import UIKit
import UniformTypeIdentifiers

@MainActor
private final class LauncherState: ObservableObject {
    static let shared = LauncherState()
    @Published var status = "launcher.loading"
    @Published var showingSettings = false
    @Published var catalog = InstanceCatalog()
    @Published var busy = false
    @Published var error: String?
    @Published var launchPending = false
#if !HORIZON_TESTFLIGHT
    private var reportedReady = false
    private var reportedVisible = false
    private var playStarted: Double?
#endif

    func appeared() {
#if !HORIZON_TESTFLIGHT
        if !reportedVisible {
            reportedVisible = true
            NSLog("MOBILE launcher_visible_ms=%.2f", horizon_ios_startup_elapsed_ms())
        }
#endif
        instances("list")
    }

    func reportReady() {
#if !HORIZON_TESTFLIGHT
        if !reportedReady && status == "launcher.ready" && catalog.current != nil && !busy {
            reportedReady = true
            NSLog("MOBILE launcher_usable_ms=%.2f", horizon_ios_startup_elapsed_ms())
        }
#endif
    }

    func receivedStatus(_ value: String) {
        status = value
        launchPending = false
#if !HORIZON_TESTFLIGHT
        if value == "gameplay.ready", let started = playStarted {
            NSLog("MOBILE play_to_controls_ms=%.2f", horizon_ios_startup_elapsed_ms() - started)
            playStarted = nil
        }
#endif
        reportReady()
    }

    private let files = DispatchQueue(label: "dev.horizon.launcher.files")

    func instances(_ operation: String, id: String? = nil, name: String? = nil, source: String? = nil) {
        guard !busy && !launchPending else { return }
        busy = true
        error = nil
        var request = ["operation": operation]
        request["id"] = id
        request["name"] = name
        request["source"] = source
        let body = request
        files.async {
            let catalog = Self.request(body)
            DispatchQueue.main.async {
                self.busy = false
                if let catalog {
                    self.catalog = catalog
                    self.reportReady()
                } else {
                    self.error = "launcher.instances_error"
                }
            }
        }
    }

    nonisolated private static func request(_ request: [String: String]) -> InstanceCatalog? {
        guard let data = try? JSONEncoder().encode(request),
              let raw = String(decoding: data, as: UTF8.self).withCString(horizon_ios_instances) else {
            return nil
        }
        defer { horizon_ios_free_string(raw) }
        return try? JSONDecoder().decode(InstanceCatalog.self, from: Data(String(cString: raw).utf8))
    }

    func play(username: String, id: String) {
        guard !busy && !launchPending else { return }
        launchPending = true
#if !HORIZON_TESTFLIGHT
        playStarted = horizon_ios_startup_elapsed_ms()
#endif
        error = nil
        files.async {
            let accepted = username.withCString { name in
                id.withCString { horizon_ios_play(name, $0) }
            }
            if accepted == 0 {
                DispatchQueue.main.async {
                    self.launchPending = false
                    self.error = "launcher.instances_error"
                }
            }
        }
    }

    func importResources(_ url: URL) {
        guard !busy && !launchPending else { return }
        busy = true
        error = nil
        files.async {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            var coordinationError: NSError?
            var result: Int32 = 2
            NSFileCoordinator().coordinate(readingItemAt: url, options: .withoutChanges, error: &coordinationError) { folder in
                result = folder.path.withCString(horizon_ios_import_resources)
            }
            let message: String?
            switch coordinationError == nil ? result : 2 {
            case 0: message = nil
            case 1: message = "launcher.resources_incompatible"
            default: message = "launcher.import_failed"
            }
            DispatchQueue.main.async {
                self.busy = false
                self.error = message
                self.reportReady()
            }
        }
    }
    fileprivate var host: UIHostingController<LauncherRootView>?
}

private struct InstanceCatalog: Decodable {
    struct Instance: Decodable, Identifiable {
        let id: String
        let name: String?
        var title: String { name ?? NSLocalizedString("launcher.instance_default", comment: "") }
    }

    var selected: String?
    var instances: [Instance] = []

    var current: Instance? { instances.first { $0.id == selected } }
}

private struct InstanceEdit: Identifiable {
    let operation: String
    let instance: InstanceCatalog.Instance?
    var id: String { operation }
}

private struct LauncherRootView: View {
    @AppStorage("offlineUsername") private var username = "Player"
    @ObservedObject private var state = LauncherState.shared
    @FocusState private var usernameFocused: Bool
    @State private var edit: InstanceEdit?
    @State private var deleting: InstanceCatalog.Instance?
    @State private var pickingResources = false

    private var working: Bool { state.busy || state.launchPending }
    private var version: String { String(cString: horizon_ios_minecraft_version()) }

    private var launcherReady: Bool {
        state.status == "launcher.ready"
    }

    var body: some View {
        Group {
            if state.status.hasPrefix("launcher.") {
                launcher
            } else if state.status.hasPrefix("gameplay.") {
                if state.status == "gameplay.loading" {
                    ZStack {
                        Color(red: 0.07, green: 0.12, blue: 0.20).ignoresSafeArea()
                        VStack(spacing: 12) {
                            ProgressView().tint(.white)
                            Text("gameplay.loading").foregroundColor(.white)
                        }
                    }
                } else {
                    gameOverlay
                }
            } else {
                Color.clear.ignoresSafeArea()
            }
        }
        .animation(.easeInOut(duration: 0.18), value: state.status)
        .onAppear { state.appeared() }
        .sheet(item: $edit) { action in
            InstanceNameSheet(action: action) { name in
                state.instances(
                    action.operation,
                    id: action.operation == "rename" ? action.instance?.id : UUID().uuidString.lowercased(),
                    name: name,
                    source: action.instance?.id
                )
            }
        }
        .alert(Text("launcher.delete_title"), isPresented: Binding(
            get: { deleting != nil },
            set: { if !$0 { deleting = nil } }
        ), presenting: deleting) { instance in
            Button("launcher.delete", role: .destructive) {
                state.instances("delete", id: instance.id)
            }
            .accessibilityIdentifier("launcher.confirm_delete")
            Button("launcher.cancel", role: .cancel) {}
        } message: { _ in
            Text("launcher.delete_message")
        }
        .fileImporter(isPresented: $pickingResources, allowedContentTypes: [.folder]) { response in
            switch response {
            case .success(let url): state.importResources(url)
            case .failure: state.error = "launcher.import_failed"
            }
        }
    }

    private var launcher: some View {
        ScrollView {
            VStack(spacing: 12) {
                Text("Horizon")
                    .font(.system(size: 42, weight: .bold, design: .rounded))
                    .foregroundColor(.white)
                Text(NSLocalizedString("launcher.subtitle", comment: "Launcher subtitle"))
                    .foregroundColor(.white.opacity(0.75))

                if state.showingSettings {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(NSLocalizedString("launcher.username", comment: "Offline username label"))
                            .foregroundColor(.white)
                        TextField("Player", text: $username)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.asciiCapable)
                            .focused($usernameFocused)
                            .submitLabel(.done)
                            .onSubmit { usernameFocused = false }
                            .padding(12)
                            .foregroundColor(Color(uiColor: .label))
                            .background(Color(uiColor: .systemBackground))
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .accessibilityIdentifier("launcher.username")
                        Button(NSLocalizedString("launcher.done", comment: "Done button")) {
                            usernameFocused = false
                            state.showingSettings = false
                        }
                        .buttonStyle(LauncherButtonStyle())
                        .accessibilityIdentifier("launcher.done")
                    }
                    .frame(maxWidth: 340)
                } else {
                    actions
                }

                if let error = state.error {
                    Text(LocalizedStringKey(error))
                        .font(.footnote)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .accessibilityIdentifier("launcher.error")
                }
            }
            .padding(16)
            .frame(maxWidth: 400)
            .frame(maxWidth: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            LinearGradient(
                colors: [Color(red: 0.07, green: 0.12, blue: 0.20), Color(red: 0.12, green: 0.23, blue: 0.30)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()
        )
    }

    private var actions: some View {
        VStack(spacing: 10) {
            HStack(spacing: 12) {
                Menu {
                    ForEach(state.catalog.instances) { instance in
                        Button {
                            state.instances("select", id: instance.id)
                        } label: {
                            if instance.id == state.catalog.selected {
                                Label(instance.title, systemImage: "checkmark")
                            } else {
                                Text(instance.title)
                            }
                        }
                        .accessibilityIdentifier("launcher.select." + instance.title)
                    }
                } label: {
                    Label(state.catalog.current?.title ?? NSLocalizedString("launcher.no_instance", comment: ""),
                          systemImage: "chevron.up.chevron.down")
                        .lineLimit(1)
                }
                .disabled(state.catalog.instances.isEmpty)
                .accessibilityIdentifier("launcher.instance")

                Menu {
                    Button("launcher.create") { edit = InstanceEdit(operation: "create", instance: nil) }
                        .accessibilityIdentifier("launcher.create")
                    if let instance = state.catalog.current {
                        Button("launcher.rename") { edit = InstanceEdit(operation: "rename", instance: instance) }
                            .accessibilityIdentifier("launcher.rename")
                        Button("launcher.duplicate") { edit = InstanceEdit(operation: "duplicate", instance: instance) }
                            .accessibilityIdentifier("launcher.duplicate")
                        Button("launcher.delete", role: .destructive) { deleting = instance }
                            .accessibilityIdentifier("launcher.delete")
                    }
                    if state.catalog.instances.isEmpty {
                        Button("launcher.reload_instances") { state.instances("list") }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle").imageScale(.large)
                }
                .accessibilityLabel(Text("launcher.manage"))
                .accessibilityIdentifier("launcher.manage")
            }
            .foregroundColor(.white)
            .padding(.vertical, 6)

            Text(String(format: NSLocalizedString("launcher.minecraft_version", comment: ""), version))
                .font(.footnote)
                .foregroundColor(.white)
            Text(LocalizedStringKey(state.status == "launcher.error" ? "launcher.ready" : state.status))
                .font(.footnote)
                .foregroundColor(.white.opacity(0.85))
                .multilineTextAlignment(.center)
                .accessibilityIdentifier("launcher.status")
            if state.status == "launcher.error" {
                Text("launcher.error").font(.footnote).foregroundColor(.white)
            }

            HStack(spacing: 12) {
                if launcherReady || state.status == "launcher.error" {
                    Button("launcher.play") {
                        guard let current = state.catalog.current else { return }
                        let name = validUsername(username)
                        username = name
                        state.play(username: name, id: current.id)
                    }
                    .buttonStyle(LauncherButtonStyle())
                    .disabled(state.catalog.current == nil)
                    .accessibilityIdentifier("launcher.play")
                }
                Button("launcher.settings") { state.showingSettings = true }
                    .buttonStyle(LauncherButtonStyle())
                    .accessibilityIdentifier("launcher.settings")
            }
            Button("launcher.import") { pickingResources = true }
                .foregroundColor(.white)
                .padding(.vertical, 6)
                .disabled(state.status == "launcher.loading")
                .accessibilityIdentifier("launcher.import")
            if working {
                ProgressView().tint(.white)
            }
        }
        .disabled(working)
    }

    private var returnButtonAlignment: Alignment {
        state.status == "gameplay.ready" ? .center : .trailing
    }

    private var gameOverlay: some View {
        VStack {
            Button(NSLocalizedString("launcher.return", comment: "Return to launcher button")) {
                horizon_ios_return_to_launcher()
            }
            .buttonStyle(LauncherButtonStyle())
            .fixedSize(horizontal: true, vertical: false)
            .disabled(state.status == "gameplay.loading" || state.status == "gameplay.stopping")
            .accessibilityIdentifier("launcher.return")
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: returnButtonAlignment)
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .ignoresSafeArea()
        .background(Color.clear)
    }
}

private struct InstanceNameSheet: View {
    @Environment(\.dismiss) private var dismiss
    let action: InstanceEdit
    let save: (String) -> Void
    @State private var name = ""

    var body: some View {
        NavigationView {
            Form {
                TextField("launcher.instance_name", text: $name)
                    .accessibilityIdentifier("launcher.instance_name")
            }
            .navigationTitle(LocalizedStringKey("launcher." + action.operation))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("launcher.cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("launcher.save") {
                        save(name)
                        dismiss()
                    }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || name.count > 64)
                    .accessibilityIdentifier("launcher.instance_save")
                }
            }
        }
        .navigationViewStyle(.stack)
        .onAppear {
            if action.operation == "rename" {
                name = action.instance?.title ?? ""
            }
        }
    }
}

private final class LauncherPassthroughView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        if LauncherState.shared.status.hasPrefix("gameplay.") {
            // Keep inventory taps clear during gameplay and center-aligned text fields clear in menus.
            let width = min(210, bounds.width / 2)
            let x = LauncherState.shared.status == "gameplay.ready"
                ? bounds.midX - width / 2 : bounds.maxX - width
            let returnButtonArea = CGRect(x: x, y: 0, width: width, height: 76)
            guard returnButtonArea.contains(point) else { return nil }
        } else if !LauncherState.shared.status.hasPrefix("launcher.") {
            return nil
        }
        return super.hitTest(point, with: event)
    }
}

private struct LauncherButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundColor(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .background(configuration.isPressed ? Color.blue.opacity(0.65) : Color.blue)
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private func validUsername(_ proposed: String) -> String {
    let value = String(proposed.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "_") }.prefix(16))
    return value.isEmpty ? "Player" : value
}

@_silgen_name("horizon_ios_play")
private func horizon_ios_play(_ username: UnsafePointer<CChar>, _ id: UnsafePointer<CChar>) -> Int32

@_silgen_name("horizon_ios_instances")
private func horizon_ios_instances(_ request: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("horizon_ios_free_string")
private func horizon_ios_free_string(_ response: UnsafeMutablePointer<CChar>)

@_silgen_name("horizon_ios_import_resources")
private func horizon_ios_import_resources(_ folder: UnsafePointer<CChar>) -> Int32

@_silgen_name("horizon_ios_minecraft_version")
private func horizon_ios_minecraft_version() -> UnsafePointer<CChar>

@_silgen_name("horizon_ios_return_to_launcher")
private func horizon_ios_return_to_launcher()

@_cdecl("horizon_ios_launcher_attach")
public func horizonIOSLauncherAttach(_ raw: UnsafeMutableRawPointer) {
    let window = Unmanaged<UIWindow>.fromOpaque(raw).takeUnretainedValue()
    DispatchQueue.main.async {
        guard let root = window.rootViewController else { return }
        let state = LauncherState.shared
        guard state.host == nil else { return }
        let host = UIHostingController(rootView: LauncherRootView())
        host.view.backgroundColor = .clear
        let container = LauncherPassthroughView(frame: root.view.bounds)
        container.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        // The renderer adds its Metal sublayer after the launcher is attached.
        container.layer.zPosition = 1
        host.view.frame = container.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        root.addChild(host)
        container.addSubview(host.view)
        root.view.addSubview(container)
        host.didMove(toParent: root)
        state.host = host
    }
}

@_cdecl("horizon_ios_launcher_status")
public func horizonIOSLauncherStatus(_ raw: UnsafePointer<CChar>?) {
    guard let raw else { return }
    let status = String(cString: raw)
    DispatchQueue.main.async {
        LauncherState.shared.receivedStatus(status)
    }
}

#if !HORIZON_TESTFLIGHT
@_silgen_name("horizon_ios_startup_elapsed_ms")
private func horizon_ios_startup_elapsed_ms() -> Double
#endif
