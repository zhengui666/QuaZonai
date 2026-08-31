import Foundation
import OSLog
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public enum EventStreamState: Sendable, Equatable {
    case disconnected
    case catchingUp
    case connected
    case reconnecting(Int)
}

public struct ServerEvent: Sendable, Equatable {
    public let id: Int
    public let name: String
    public let data: JSONValue
}

public actor EventStreamActor {
    private static let logger = Logger(subsystem: "QuaZonai", category: "EventStream")

    private let client: APIClient
    private let session: URLSession
    private let persistCursor: @Sendable (Int) async -> Void
    private var task: Task<Void, Never>?
    private var eventCursor: EventSequenceCursor

    public init(
        client: APIClient,
        cursor: Int = 0,
        session: URLSession = .shared,
        persistCursor: @escaping @Sendable (Int) async -> Void = { _ in }
    ) {
        self.client = client
        self.eventCursor = EventSequenceCursor(cursor)
        self.session = session
        self.persistCursor = persistCursor
    }

    deinit { task?.cancel() }

    public func start(
        onState: @escaping @Sendable (EventStreamState) async -> Void,
        onEvent: @escaping @Sendable (ServerEvent) async -> Void
    ) {
        guard task == nil else { return }
        task = Task { [weak self] in
            guard let self else { return }
            await self.run(onState: onState, onEvent: onEvent)
        }
    }

    public func stop() {
        task?.cancel()
        task = nil
    }

    private func run(
        onState: @escaping @Sendable (EventStreamState) async -> Void,
        onEvent: @escaping @Sendable (ServerEvent) async -> Void
    ) async {
        var attempt = 0
        while !Task.isCancelled {
            do {
                await onState(.catchingUp)
                try await catchUp(onEvent: onEvent)
                await onState(.connected)
                try await stream(onEvent: onEvent)
                attempt = 0
            } catch {
                if Task.isCancelled { return }
                attempt += 1
                Self.logger.error("Event stream disconnected; scheduling reconnect attempt \(attempt)")
                await onState(.reconnecting(attempt))
                if case APIClientError.authenticationRequired = error {
                    _ = try? await client.refreshIfPossible()
                }
                let delay = EventStreamBackoff.delaySeconds(
                    attempt: attempt,
                    jitterUnit: Double.random(in: 0...1)
                )
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    private func catchUp(onEvent: @escaping @Sendable (ServerEvent) async -> Void) async throws {
        let result = try await client.requestJSON(
            path: "/api/v1/events",
            queryItems: [
                URLQueryItem(name: "after_id", value: String(eventCursor.value)),
                URLQueryItem(name: "limit", value: "1000"),
            ]
        )
        guard let items = result.arrayValue else { return }
        for item in items {
            guard let object = item.objectValue,
                  let idValue = object.number("id") else { continue }
            let id = Int(idValue)
            guard eventCursor.accept(id) else { continue }
            await persistCursor(id)
            await onEvent(
                ServerEvent(
                    id: id,
                    name: object.string("kind") ?? "qz-event",
                    data: item
                )
            )
        }
    }

    private func stream(onEvent: @escaping @Sendable (ServerEvent) async -> Void) async throws {
        var request = try await client.authorizedRequest(
            path: "/api/v1/events/stream",
            queryItems: [URLQueryItem(name: "cursor", value: String(eventCursor.value))]
        )
        if eventCursor.value > 0 {
            request.setValue(String(eventCursor.value), forHTTPHeaderField: "Last-Event-ID")
        }
        let (bytes, rawResponse) = try await session.bytes(for: request)
        guard let response = rawResponse as? HTTPURLResponse else {
            throw APIClientError.invalidResponse
        }
        guard (200..<300).contains(response.statusCode) else {
            if response.statusCode == 401 { throw APIClientError.authenticationRequired }
            throw APIClientError.http(
                status: response.statusCode,
                code: nil,
                message: "Event stream failed"
            )
        }

        var parser = SSEFrameParser()
        for try await line in bytes.lines {
            if Task.isCancelled { return }
            guard let frame = parser.consume(line: line),
                  let id = frame.id,
                  eventCursor.accept(id) else { continue }
            let value = try JSONDecoder().decode(JSONValue.self, from: Data(frame.data.utf8))
            await persistCursor(id)
            await onEvent(ServerEvent(id: id, name: frame.event, data: value))
        }
        throw URLError(.networkConnectionLost)
    }
}
