import Charts
import SwiftUI

struct JSONDocumentView: View {
    let value: JSONValue

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let series = NumericSeries.first(in: value), series.count >= 2 {
                Chart(Array(series.enumerated()), id: \.offset) { index, point in
                    LineMark(x: .value("Index", index), y: .value(point.label, point.value))
                }
                .frame(minHeight: 180)
                .accessibilityLabel(NumericSeries.summary(series))
            }
            JSONTreeView(value: value)
        }
    }
}

struct JSONTreeView: View {
    let value: JSONValue

    var body: some View {
        switch value {
        case let .object(object):
            VStack(alignment: .leading, spacing: 8) {
                ForEach(object.keys.sorted(), id: \.self) { key in
                    if let child = object[key] {
                        if child.objectValue != nil || child.arrayValue != nil {
                            DisclosureGroup(key.replacingOccurrences(of: "_", with: " ").capitalized) {
                                JSONTreeView(value: child).padding(.leading, 8)
                            }
                        } else {
                            LabeledContent(key.replacingOccurrences(of: "_", with: " ").capitalized) {
                                Text(child.scalarDescription).textSelection(.enabled).multilineTextAlignment(.trailing)
                            }
                        }
                    }
                }
            }
        case let .array(array):
            if array.isEmpty { Text("—").foregroundStyle(.secondary) }
            else {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(array.enumerated()), id: \.offset) { index, item in
                        DisclosureGroup("#\(index + 1)") { JSONTreeView(value: item).padding(.leading, 8) }
                    }
                }
            }
        default:
            Text(value.scalarDescription).textSelection(.enabled)
        }
    }
}

private struct NumericPoint {
    let label: String
    let value: Double
}

private enum NumericSeries {
    private static let preferredKeys = ["equity", "benchmark", "close", "drawdown", "value", "score", "return"]

    static func first(in value: JSONValue) -> [NumericPoint]? {
        if let array = value.arrayValue {
            for key in preferredKeys {
                let points = array.compactMap { item -> NumericPoint? in
                    guard let object = item.objectValue, let number = object.number(key) else { return nil }
                    return NumericPoint(label: key, value: number)
                }
                if points.count >= 2 { return points }
            }
            for child in array {
                if let result = first(in: child) { return result }
            }
        }
        if let object = value.objectValue {
            for child in object.values {
                if let result = first(in: child) { return result }
            }
        }
        return nil
    }

    static func summary(_ series: [NumericPoint]) -> String {
        guard let first = series.first, let last = series.last else { return "Chart" }
        let values = series.map(\.value)
        return "\(first.label) chart. Start \(first.value), end \(last.value), minimum \(values.min() ?? 0), maximum \(values.max() ?? 0)."
    }
}

extension JSONValue {
    var normalizedItems: [JSONValue] {
        if let arrayValue { return arrayValue }
        if let items = self["items"]?.arrayValue { return items }
        return []
    }

    var stableID: String? { objectValue?.string("id") }
    var listTitle: String {
        guard let object = objectValue else { return scalarDescription }
        for key in ["title", "name", "research_question", "plugin_id", "id"] {
            if let value = object.string(key), !value.isEmpty { return value }
        }
        return "Record"
    }

    var searchableText: String {
        switch self {
        case let .string(value): value
        case let .number(value): String(value)
        case let .bool(value): String(value)
        case .null: ""
        case let .array(value): value.map(\.searchableText).joined(separator: " ")
        case let .object(value): value.map { "\($0.key) \($0.value.searchableText)" }.joined(separator: " ")
        }
    }
}
