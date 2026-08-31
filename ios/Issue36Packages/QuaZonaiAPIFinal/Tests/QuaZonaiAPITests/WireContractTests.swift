import Testing
@testable import QuaZonaiAPI

@Test("Generated wire contract exposes the native capability epoch")
func capabilityEpoch() {
    #expect(QuaZonaiWireContract.capabilityEpoch == 1)
}
