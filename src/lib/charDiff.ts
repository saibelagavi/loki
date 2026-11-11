// Character-level diff for two revealed values, so you can see exactly where they differ
// (a missing slash, a trailing space, one changed character in a host name).
// Common prefix and suffix are trimmed first; the middle uses an LCS table, capped so a huge
// value never freezes the UI (beyond the cap the whole middle is shown as replaced).

export interface DiffPart {
  type: "same" | "del" | "add";
  text: string;
}

const MAX_CELLS = 250_000;

function push(parts: DiffPart[], type: DiffPart["type"], text: string) {
  if (!text) return;
  const last = parts[parts.length - 1];
  if (last && last.type === type) last.text += text;
  else parts.push({ type, text });
}

export function charDiff(a: string, b: string): DiffPart[] {
  if (a === b) return a ? [{ type: "same", text: a }] : [];
  const A = Array.from(a);
  const B = Array.from(b);
  let start = 0;
  while (start < A.length && start < B.length && A[start] === B[start]) start++;
  let endA = A.length;
  let endB = B.length;
  while (endA > start && endB > start && A[endA - 1] === B[endB - 1]) {
    endA--;
    endB--;
  }
  const parts: DiffPart[] = [];
  push(parts, "same", A.slice(0, start).join(""));

  const midA = A.slice(start, endA);
  const midB = B.slice(start, endB);
  const n = midA.length;
  const m = midB.length;
  if (n * m > MAX_CELLS) {
    push(parts, "del", midA.join(""));
    push(parts, "add", midB.join(""));
  } else {
    // lcs[i][j] = LCS length of midA[i..] and midB[j..]
    const lcs: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        lcs[i][j] = midA[i] === midB[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (midA[i] === midB[j]) {
        push(parts, "same", midA[i]);
        i++;
        j++;
      } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
        push(parts, "del", midA[i++]);
      } else {
        push(parts, "add", midB[j++]);
      }
    }
    push(parts, "del", midA.slice(i).join(""));
    push(parts, "add", midB.slice(j).join(""));
  }

  push(parts, "same", A.slice(endA).join(""));
  return parts;
}

/** Rebuild either side from the parts (used by tests and for copy-free display checks). */
export function side(parts: DiffPart[], which: "left" | "right"): string {
  return parts
    .filter((p) => p.type === "same" || p.type === (which === "left" ? "del" : "add"))
    .map((p) => p.text)
    .join("");
}

/** Make invisible characters visible inside a revealed value: spaces at the ends, tabs, newlines. */
export function visible(text: string): string {
  return text.replace(/\t/g, "→").replace(/\r/g, "␍").replace(/\n/g, "↵").replace(/^ +| +$/g, (m) => "·".repeat(m.length));
}
