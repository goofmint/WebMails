/**
 * Shown instead of the service list when `Snapshot.configError` is set
 * (Task 1.10). Read-only: no repair, retry or fallback-to-defaults action
 * is implemented here (project rule: no fallback defaults) — fixing
 * `config.toml` is the only way past this screen.
 */

import type { ConfigErrorInfo } from "../ipc";

export interface ConfigErrorScreenProps {
  readonly configError: ConfigErrorInfo;
  /** Sidebar width in px, from `Snapshot.sidebarWidth` — never hard-coded. */
  readonly sidebarWidth: number;
  readonly onOpenSettings: () => void;
}

export function ConfigErrorScreen({
  configError,
  sidebarWidth,
  onOpenSettings,
}: ConfigErrorScreenProps) {
  const fullText =
    configError.key === null
      ? `${configError.file}\n${configError.reason}`
      : `${configError.file}\n${configError.key}\n${configError.reason}`;

  return (
    <div className="config-error-screen" style={{ width: sidebarWidth }} title={fullText}>
      <p className="config-error-screen__file">{configError.file}</p>
      {configError.key !== null && <p className="config-error-screen__key">{configError.key}</p>}
      <p className="config-error-screen__reason">{configError.reason}</p>
      <button
        type="button"
        className="sidebar__settings-button"
        aria-label="Settings"
        onClick={onOpenSettings}
      >
        ⚙
      </button>
    </div>
  );
}
