import { useId, useState } from "react";
import { passwordStrength } from "../lib/strength";

interface Props {
  label: string;
  value: string;
  onChange: (v: string) => void;
  autoFocus?: boolean;
  showStrength?: boolean;
  disabled?: boolean;
  autoComplete?: string;
}

const BAR = ["var(--prod)", "var(--prod)", "var(--staging)", "var(--dev)", "var(--dev)"];

export function PasswordField({ label, value, onChange, autoFocus, showStrength, disabled, autoComplete = "off" }: Props) {
  const id = useId();
  const [visible, setVisible] = useState(false);
  const s = passwordStrength(value);

  return (
    <div>
      <label htmlFor={id} className="label">
        {label}
      </label>
      <div className="flex gap-2">
        <input
          id={id}
          className="input"
          type={visible ? "text" : "password"}
          value={value}
          autoFocus={autoFocus}
          disabled={disabled}
          autoComplete={autoComplete}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
        />
        <button type="button" className="btn" onClick={() => setVisible((v) => !v)} aria-pressed={visible} disabled={disabled}>
          {visible ? "Hide" : "Show"}
        </button>
      </div>
      {showStrength && (
        <div className="mt-2" aria-live="polite">
          <div className="flex h-1 gap-1" aria-hidden>
            {[0, 1, 2, 3].map((i) => (
              <span key={i} className="h-full flex-1 rounded-full" style={{ background: value && s.score > i ? BAR[s.score] : "var(--line)" }} />
            ))}
          </div>
          <p className="mt-1 text-[12px] text-muted">
            {s.label && <strong className="mr-1 font-medium text-chalk">{s.label}.</strong>}
            {s.hint}
          </p>
        </div>
      )}
    </div>
  );
}
