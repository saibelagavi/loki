import { createContext, useContext } from "react";
import type { Settings } from "../types";

export const DEFAULT_SETTINGS: Settings = {
  autoLockMinutes: 5,
  lockOnMinimize: false,
  clipboardClearSeconds: 30,
  revealSeconds: 10,
  excludeFromCapture: false,
  disabledRules: [],
  customRules: [],
  diffRevealSeconds: 60,
  rotationDays: 90,
};

interface Value {
  settings: Settings;
  setSettings: (s: Settings) => void;
}

export const SettingsContext = createContext<Value>({ settings: DEFAULT_SETTINGS, setSettings: () => undefined });
export const useSettings = () => useContext(SettingsContext);
