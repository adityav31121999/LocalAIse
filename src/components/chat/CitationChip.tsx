import { baseName } from "../../util/format";
import type { Citation } from "../../types";

export function CitationChip({ citation }: { citation: Citation }) {
  const detail = `${citation.fileName}${citation.page ? ` · page ${citation.page}` : ""}`;

  return (
    <span className="citation" title={detail}>
      <span className="citation__index">[{citation.index}]</span>
      <span className="truncate">{baseName(citation.fileName)}</span>
      {citation.page ? <span className="citation__page">p.{citation.page}</span> : null}
    </span>
  );
}
