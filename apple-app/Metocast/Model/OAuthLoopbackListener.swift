import Foundation
import Network

/// Listens on loopback (127.0.0.1:8766) to capture the OAuth redirect callback
/// when signing in to YouTube or Facebook from a client device.
@MainActor
final class OAuthLoopbackListener {
    struct CallbackResult {
        let code: String
        let state: String
    }

    enum ListenerError: LocalizedError {
        case portInUse
        case cancelled
        case timedOut
        case invalidCallback

        var errorDescription: String? {
            switch self {
            case .portInUse:
                return String(localized: "Port 8766 is already in use.")
            case .cancelled:
                return String(localized: "Sign-in was cancelled.")
            case .timedOut:
                return String(localized: "Sign-in timed out. Please try again.")
            case .invalidCallback:
                return String(localized: "The authorization response was invalid or was denied.")
            }
        }
    }

    static let port: UInt16 = 8766
    private var listener: NWListener?
    private var connection: NWConnection?
    private var continuation: CheckedContinuation<CallbackResult, Error>?
    private var isCompleted = false

    func start() throws {
        let parameters = NWParameters.tcp
        parameters.allowLocalEndpointReuse = true
        parameters.requiredInterfaceType = .loopback
        guard let nwPort = NWEndpoint.Port(rawValue: Self.port) else {
            throw ListenerError.portInUse
        }
        let listener = try NWListener(using: parameters, on: nwPort)
        self.listener = listener

        listener.newConnectionHandler = { [weak self] connection in
            Task { @MainActor [weak self] in
                self?.handleConnection(connection)
            }
        }
        listener.stateUpdateHandler = { [weak self] state in
            if case .failed = state {
                Task { @MainActor [weak self] in
                    self?.fail(ListenerError.portInUse)
                }
            }
        }
        listener.start(queue: .main)
    }

    func waitForCallback(timeoutSeconds: TimeInterval = 180) async throws -> CallbackResult {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                self.continuation = continuation
                Task { [weak self] in
                    try? await Task.sleep(nanoseconds: UInt64(timeoutSeconds * 1_000_000_000))
                    self?.fail(ListenerError.timedOut)
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.cancel()
            }
        }
    }

    func cancel() {
        fail(ListenerError.cancelled)
    }

    private func handleConnection(_ connection: NWConnection) {
        self.connection = connection
        connection.start(queue: .main)
        readHTTPRequest(connection: connection)
    }

    private func readHTTPRequest(connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 4096) { [weak self] data, _, isComplete, error in
            Task { @MainActor [weak self] in
                guard let self else { return }
                if let data, let requestString = String(data: data, encoding: .utf8) {
                    self.processRequest(requestString, on: connection)
                } else if error != nil || isComplete {
                    connection.cancel()
                }
            }
        }
    }

    private func processRequest(_ request: String, on connection: NWConnection) {
        guard let firstLine = request.components(separatedBy: "\r\n").first,
              let pathWithQuery = firstLine.split(separator: " ").dropFirst().first else {
            connection.cancel()
            return
        }

        let urlString = "http://127.0.0.1:8766" + pathWithQuery
        guard let components = URLComponents(string: urlString) else {
            connection.cancel()
            return
        }

        // A browser also asks for things like /favicon.ico; those mustn't end the sign-in.
        guard components.path == "/callback" else {
            let notFound = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            connection.send(content: Data(notFound.utf8), completion: .contentProcessed { _ in connection.cancel() })
            return
        }

        let queryItems = components.queryItems ?? []
        let code = queryItems.first { $0.name == "code" }?.value
        let state = queryItems.first { $0.name == "state" }?.value
        let error = queryItems.first { $0.name == "error" }?.value

        let responseBody: String
        let success: Bool

        if code != nil, state != nil, error == nil {
            success = true
            responseBody = """
            <!DOCTYPE html>
            <html lang="en">
            <head>
              <meta charset="UTF-8">
              <meta name="viewport" content="width=device-width, initial-scale=1.0">
              <title>Authentication Complete</title>
              <style>
                body { font-family: -apple-system, system-ui, sans-serif; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; background: #f9fafb; }
                .card { text-align: center; padding: 2.5rem 2rem; background: #fff; border-radius: 1rem; box-shadow: 0 4px 12px rgba(0,0,0,.08); max-width: 380px; margin: 1rem; }
                h1 { color: #166534; font-size: 1.4rem; margin-bottom: 0.5rem; }
                p { color: #4b5563; font-size: 0.95rem; line-height: 1.4; margin: 0; }
              </style>
            </head>
            <body>
              <div class="card">
                <h1>Authentication Complete</h1>
                <p>You can close this tab and return to Metocast.</p>
              </div>
            </body>
            </html>
            """
        } else {
            success = false
            responseBody = """
            <!DOCTYPE html>
            <html lang="en">
            <head>
              <meta charset="UTF-8">
              <title>Authentication Failed</title>
              <style>
                body { font-family: -apple-system, system-ui, sans-serif; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; background: #f9fafb; }
                .card { text-align: center; padding: 2.5rem 2rem; background: #fff; border-radius: 1rem; box-shadow: 0 4px 12px rgba(0,0,0,.08); max-width: 380px; margin: 1rem; }
                h1 { color: #991b1b; font-size: 1.4rem; margin-bottom: 0.5rem; }
                p { color: #4b5563; font-size: 0.95rem; margin: 0; }
              </style>
            </head>
            <body>
              <div class="card">
                <h1>Authentication Failed</h1>
                <p>The sign-in request was rejected or an error occurred. Please try again.</p>
              </div>
            </body>
            </html>
            """
        }

        let httpResponse = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: \(responseBody.utf8.count)\r\n\r\n\(responseBody)"
        let data = Data(httpResponse.utf8)

        connection.send(content: data, completion: .contentProcessed({ [weak self] _ in
            Task { @MainActor [weak self] in
                connection.cancel()
                if success, let code, let state {
                    self?.succeed(CallbackResult(code: code, state: state))
                } else {
                    self?.fail(ListenerError.invalidCallback)
                }
            }
        }))
    }

    private func succeed(_ result: CallbackResult) {
        guard !isCompleted else { return }
        isCompleted = true
        listener?.cancel()
        listener = nil
        continuation?.resume(returning: result)
        continuation = nil
    }

    private func fail(_ error: Error) {
        guard !isCompleted else { return }
        isCompleted = true
        listener?.cancel()
        listener = nil
        continuation?.resume(throwing: error)
        continuation = nil
    }

    deinit {
        listener?.cancel()
        connection?.cancel()
    }
}
