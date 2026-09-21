export type DraftSummary = {
  writer: string;
  updatedAt: string;
  name: string;
  baseKey?: string | null;
  key: string;
};

export function RecoveryDrafts({
  drafts,
  restore,
}: {
  drafts: DraftSummary[];
  restore: (writer: string) => void;
}) {
  if (!drafts.length) return null;
  return (
    <details className="recovery-drafts">
      <summary>
        {drafts.length} recovery {drafts.length === 1 ? "draft" : "drafts"}
      </summary>
      <div className="recovery-draft-menu">
        <p>
          Unsaved work from other windows is preserved. Opening a draft replaces the current preview; it does
          not publish or overwrite saved work.
        </p>
        {drafts.map((draft) => (
          <button type="button" key={draft.writer} onClick={() => restore(draft.writer)}>
            <strong>{draft.name}</strong>
            <span>{new Date(draft.updatedAt).toLocaleString()}</span>
          </button>
        ))}
      </div>
    </details>
  );
}
