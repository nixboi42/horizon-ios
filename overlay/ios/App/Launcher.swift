import SwiftUI
import UIKit

@MainActor
private final class LauncherState: ObservableObject {
    static let shared = LauncherState()
    @Published var status = "launcher.ready"
    @Published var showingSettings = false
    fileprivate var host: UIHostingController<LauncherRootView>?
}

private struct LauncherRootView: View {
    @AppStorage("offlineUsername") private var username = "Player"
    @ObservedObject private var state = LauncherState.shared
    @FocusState private var usernameFocused: Bool

    private var launcherReady: Bool {
        state.status == "launcher.ready"
    }

    var body: some View {
        Group {
            if state.status.hasPrefix("launcher.") {
                launcher
            } else if state.status.hasPrefix("gameplay.") {
                gameOverlay
            } else {
                Color.clear.ignoresSafeArea()
            }
        }
        .animation(.easeInOut(duration: 0.18), value: state.status)
    }

    private var launcher: some View {
        VStack(spacing: 20) {
            Spacer()
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
                VStack(spacing: 12) {
                    Button(NSLocalizedString("launcher.play", comment: "Play button")) {
                        usernameFocused = false
                        let name = validUsername(username)
                        username = name
                        name.withCString { horizon_ios_play($0) }
                    }
                    .buttonStyle(LauncherButtonStyle())
                    .disabled(!launcherReady)
                    .accessibilityIdentifier("launcher.play")

                    Button(NSLocalizedString("launcher.settings", comment: "Settings button")) {
                        state.showingSettings = true
                    }
                    .buttonStyle(LauncherButtonStyle())
                    .accessibilityIdentifier("launcher.settings")
                }
                .frame(maxWidth: 280)
            }

            if !launcherReady {
                Text(NSLocalizedString(state.status, comment: "Launcher status"))
                    .font(.footnote)
                    .foregroundColor(.white.opacity(0.8))
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 28)
                    .accessibilityIdentifier("launcher.status")
            }
            Spacer()
        }
        .padding(24)
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

    private var gameOverlay: some View {
        VStack {
            HStack {
                Spacer()
                Button(NSLocalizedString("launcher.return", comment: "Return to launcher button")) {
                    horizon_ios_return_to_launcher()
                }
                .buttonStyle(LauncherButtonStyle())
                .fixedSize(horizontal: true, vertical: false)
                .disabled(state.status == "gameplay.loading" || state.status == "gameplay.stopping")
                .accessibilityIdentifier("launcher.return")
            }
            Spacer()
        }
        .padding(.top, 8)
        .padding(.trailing, 12)
        .ignoresSafeArea()
        .background(Color.clear)
    }
}

private final class LauncherPassthroughView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        if LauncherState.shared.status.hasPrefix("gameplay.") {
            let returnButtonArea = CGRect(x: bounds.width - 210, y: 0, width: 210, height: 76)
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
private func horizon_ios_play(_ username: UnsafePointer<CChar>)

@_silgen_name("horizon_ios_return_to_launcher")
private func horizon_ios_return_to_launcher()

@_cdecl("horizon_ios_launcher_attach")
public func horizonIOSLauncherAttach() {
    DispatchQueue.main.async {
        guard let window = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .flatMap({ $0.windows })
            .first(where: { $0.isKeyWindow }),
              let root = window.rootViewController else { return }
        let state = LauncherState.shared
        guard state.host == nil else { return }
        let host = UIHostingController(rootView: LauncherRootView())
        host.view.backgroundColor = .clear
        let container = LauncherPassthroughView(frame: root.view.bounds)
        container.autoresizingMask = [.flexibleWidth, .flexibleHeight]
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
        LauncherState.shared.status = status
    }
}
