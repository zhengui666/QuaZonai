import Testing
@testable import QuaZonaiAPI

@Test("Generated client package exports the capability epoch")
func capabilityEpoch() {
    #expect(QuaZonaiWireContract.capabilityEpoch == 1)
}
