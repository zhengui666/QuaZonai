import Foundation
import OpenAPIURLSession

/// Constructs the Swift OpenAPI generated client for the same server used by the
/// native transport. Domain screens preserve unknown JSON fields for parity, while
/// this generated client keeps the canonical wire contract compiled into the app.
public func makeGeneratedClient(serverURL: URL) -> Client {
    Client(serverURL: serverURL, transport: URLSessionTransport())
}
