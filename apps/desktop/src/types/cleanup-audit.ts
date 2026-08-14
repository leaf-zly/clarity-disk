/** Privacy-preserving cleanup audit events, including restricted execution. */
export interface CleanupAuditEvent {
  eventId: string;
  kind:
    | "scanCompleted"
    | "planCreated"
    | "planRejected"
    | "quarantineIndexCreated"
    | "executionConfirmationIssued"
    | "executionStarted"
    | "executionCompleted"
    | "quarantineRestoreStarted"
    | "quarantineRestoreCompleted"
    | "quarantineBatchRestoreStarted"
    | "quarantineBatchRestoreCompleted"
    | "quarantinePolicyUpdated";
  subjectId: string;
  occurredAtUnixMs: number;
  reason: string | null;
  candidateCount: number;
  totalBytes: number;
  ruleIds: string[];
}
