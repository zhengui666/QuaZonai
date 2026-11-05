/** Server-configured source-owner documents. These descriptors are never accepted from an Agent or ResearchInput. */
export interface ReviewSourceFiles {
  metadata: { path: string; sha256: string };
  license: { path: string; sha256: string };
  heldoutMetadata?: { path: string; sha256: string };
  heldoutLicense?: { path: string; sha256: string };
}
/** Original source-owner authorization, bound to one immutable snapshot and native dataset identity. */
export interface ReviewSourceLicense {
  schemaVersion: 1; datasetId: string; sourceId: string; sourceRevision: string;
  datasetRevisionId: string; grantId: string; issuer: string; reference: string;
  status: 'ACTIVE' | 'REVOKED'; purposes: ('RESEARCH' | 'REPORT')[];
  validFromNs: string; validUntilNs: string;
}
