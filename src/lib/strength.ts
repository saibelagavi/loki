// A small, offline password strength hint. It guides; the core only enforces a minimum length.

export interface Strength {
  score: 0 | 1 | 2 | 3 | 4;
  label: string;
  hint: string;
}

const COMMON = ["password", "123456", "qwerty", "letmein", "welcome", "admin", "iloveyou", "abc123", "loki"];

export function passwordStrength(pw: string): Strength {
  if (pw.length === 0) return { score: 0, label: "", hint: "Use at least 8 characters. A few random words work well." };
  const lower = pw.toLowerCase();
  const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter((r) => r.test(pw)).length;
  let points = 0;
  if (pw.length >= 8) points += 1;
  if (pw.length >= 12) points += 1;
  if (pw.length >= 16) points += 1;
  if (classes >= 3) points += 1;
  if (new Set(pw).size < Math.max(4, pw.length / 3)) points -= 1; // aaaaaaaa
  if (COMMON.some((c) => lower.includes(c))) points -= 2;
  if (pw.length < 8) return { score: 0, label: "Too short", hint: "Use at least 8 characters." };

  const score = Math.max(0, Math.min(4, points)) as Strength["score"];
  const table: Record<number, Pick<Strength, "label" | "hint">> = {
    0: { label: "Weak", hint: "Avoid common words and repeated characters." },
    1: { label: "Fair", hint: "Longer is better. Try 12 or more characters." },
    2: { label: "Good", hint: "Add more length or a few random words." },
    3: { label: "Strong", hint: "Nice. Remember it: LOKI cannot recover it." },
    4: { label: "Very strong", hint: "Excellent. Remember it: LOKI cannot recover it." },
  };
  return { score, ...table[score] };
}
