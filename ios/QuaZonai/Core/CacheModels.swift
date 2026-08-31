import Foundation
import SwiftData

@Model
final class CachedPayload {
    @Attribute(.unique) var cacheKey: String
    var serverProfile: String
    var payload: Data
    var updatedAt: Date

    init(cacheKey: String, serverProfile: String, payload: Data, updatedAt: Date = .now) {
        self.cacheKey = cacheKey
        self.serverProfile = serverProfile
        self.payload = payload
        self.updatedAt = updatedAt
    }
}

@Model
final class IdeaDraft {
    @Attribute(.unique) var draftKey: String
    var serverProfile: String
    var text: String
    var updatedAt: Date

    init(serverProfile: String, text: String = "") {
        self.serverProfile = serverProfile
        self.draftKey = "\(serverProfile)|idea-draft"
        self.text = text
        self.updatedAt = .now
    }
}

@Model
final class EventCursor {
    @Attribute(.unique) var cursorKey: String
    var serverProfile: String
    var lastEventID: Int
    var updatedAt: Date

    init(serverProfile: String, lastEventID: Int = 0) {
        self.serverProfile = serverProfile
        self.cursorKey = "\(serverProfile)|event-cursor"
        self.lastEventID = lastEventID
        self.updatedAt = .now
    }
}

@MainActor
final class CacheStore {
    private let context: ModelContext

    init(context: ModelContext) { self.context = context }

    func save(_ value: JSONValue, key: String, profile: String) throws {
        let cacheKey = "\(profile)|\(key)"
        let descriptor = FetchDescriptor<CachedPayload>(predicate: #Predicate { $0.cacheKey == cacheKey })
        let data = try value.encodedData()
        if let existing = try context.fetch(descriptor).first {
            existing.payload = data
            existing.updatedAt = .now
        } else {
            context.insert(CachedPayload(cacheKey: cacheKey, serverProfile: profile, payload: data))
        }
        try context.save()
    }

    func load(key: String, profile: String) -> JSONValue? {
        let cacheKey = "\(profile)|\(key)"
        let descriptor = FetchDescriptor<CachedPayload>(predicate: #Predicate { $0.cacheKey == cacheKey })
        guard let cached = try? context.fetch(descriptor).first else { return nil }
        return try? JSONDecoder().decode(JSONValue.self, from: cached.payload)
    }

    func cursor(profile: String) -> Int {
        let key = "\(profile)|event-cursor"
        let descriptor = FetchDescriptor<EventCursor>(predicate: #Predicate { $0.cursorKey == key })
        return (try? context.fetch(descriptor).first?.lastEventID) ?? 0
    }

    func saveCursor(_ value: Int, profile: String) {
        let key = "\(profile)|event-cursor"
        let descriptor = FetchDescriptor<EventCursor>(predicate: #Predicate { $0.cursorKey == key })
        if let existing = try? context.fetch(descriptor).first {
            existing.lastEventID = max(existing.lastEventID, value)
            existing.updatedAt = .now
        } else {
            context.insert(EventCursor(serverProfile: profile, lastEventID: value))
        }
        try? context.save()
    }
}
