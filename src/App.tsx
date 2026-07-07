import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

// ── Types ─────────────────────────────────────────────────────────────────────

type Config = {
  app_url: string;
  device_id: string | null;
  run_root: string | null;
  enabled: boolean;
};

type RunFileStatus = {
  path: string;
  status: "uploaded" | "duplicate" | "error";
  error?: string;
};

type SyncResult = {
  imported: number;
  duplicates: number;
  errors: number;
  files: RunFileStatus[];
};

// ── App ───────────────────────────────────────────────────────────────────────

export default function App() {
  const [token, setToken] = useState<string | null>(null);
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    const [t, c] = await Promise.all([
      invoke<string | null>("get_token"),
      invoke<Config>("get_config"),
    ]);
    setToken(t);
    setConfig(c);
    setLoading(false);
  }, []);

  useEffect(() => {
    reload().catch(console.error);
  }, [reload]);

  if (loading || config === null) {
    return (
      <div className="min-h-screen bg-zinc-950 flex items-center justify-center">
        <p className="text-zinc-500 text-sm">Loading…</p>
      </div>
    );
  }

  if (token === null) {
    return <NotPairedView config={config} onPaired={reload} />;
  }

  return <PairedView config={config} onUnpaired={reload} onConfigChange={reload} />;
}

// ── Not-paired view ───────────────────────────────────────────────────────────

function NotPairedView({
  config,
  onPaired,
}: {
  config: Config;
  onPaired: () => void;
}) {
  const [pairCode, setPairCode] = useState("");
  const [dashboardUrl, setDashboardUrl] = useState(
    config.app_url || "https://sts2-dashboard.lemoncode.dev"
  );
  const [pairing, setPairing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleOpenDashboard() {
    await openUrl("https://sts2-dashboard.lemoncode.dev/");
  }

  async function handlePair(e: React.FormEvent) {
    e.preventDefault();
    if (!pairCode.trim()) return;
    setPairing(true);
    setError(null);
    try {
      await invoke("pair_device", { code: pairCode.trim(), appUrl: dashboardUrl.trim() });
      onPaired();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPairing(false);
    }
  }

  return (
    <div className="min-h-screen bg-zinc-950 flex items-start justify-center pt-12 px-6">
      <div className="w-full max-w-sm">
        {/* Header */}
        <h1 className="text-zinc-100 text-xl font-semibold mb-1">STS2 Sync</h1>
        <p className="text-zinc-400 text-sm mb-8">
          Connect this app to your STS2 Dashboard to automatically upload your
          run files.
        </p>

        {/* Primary CTA */}
        <button
          onClick={handleOpenDashboard}
          className="w-full bg-indigo-600 hover:bg-indigo-500 active:bg-indigo-700 text-white text-sm font-medium py-2.5 px-4 rounded-lg transition-colors mb-8"
        >
          Connect to Dashboard →
        </button>

        {/* Divider */}
        <div className="flex items-center gap-3 mb-6">
          <div className="flex-1 h-px bg-zinc-800" />
          <span className="text-zinc-600 text-xs">or enter your pair code</span>
          <div className="flex-1 h-px bg-zinc-800" />
        </div>

        {/* Manual pair form */}
        <form onSubmit={handlePair} className="space-y-3">
          <div>
            <label className="block text-zinc-400 text-xs mb-1">Pair code</label>
            <input
              type="text"
              value={pairCode}
              onChange={(e) => setPairCode(e.target.value)}
              placeholder="e.g. ABC123"
              className="w-full bg-zinc-900 border border-zinc-700 focus:border-indigo-500 text-zinc-100 text-sm rounded-lg px-3 py-2 outline-none transition-colors placeholder:text-zinc-600"
            />
          </div>
          <div>
            <label className="block text-zinc-400 text-xs mb-1">Dashboard URL</label>
            <input
              type="url"
              value={dashboardUrl}
              onChange={(e) => setDashboardUrl(e.target.value)}
              className="w-full bg-zinc-900 border border-zinc-700 focus:border-indigo-500 text-zinc-100 text-sm rounded-lg px-3 py-2 outline-none transition-colors"
            />
          </div>
          <button
            type="submit"
            disabled={pairing || !pairCode.trim()}
            className="w-full bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed text-zinc-100 text-sm font-medium py-2.5 px-4 rounded-lg transition-colors"
          >
            {pairing ? "Pairing…" : "Pair"}
          </button>
        </form>

        {error && (
          <p className="mt-4 text-red-400 text-sm bg-red-950/40 border border-red-900/50 rounded-lg px-3 py-2">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

// ── Paired / main view ────────────────────────────────────────────────────────

function PairedView({
  config,
  onUnpaired,
  onConfigChange,
}: {
  config: Config;
  onUnpaired: () => void;
  onConfigChange: () => void;
}) {
  const [enabled, setEnabled] = useState(config.enabled);
  const [runRoot, setRunRoot] = useState(config.run_root);
  const [syncing, setSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState<SyncResult | null>(null);
  const [lastSynced, setLastSynced] = useState<Date | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [toggleError, setToggleError] = useState<string | null>(null);

  // Extract hostname from config url for display
  const dashboardHost = (() => {
    try {
      return new URL(config.app_url).hostname;
    } catch {
      return config.app_url;
    }
  })();

  async function handleToggleEnabled(next: boolean) {
    setEnabled(next);
    setToggleError(null);
    try {
      await invoke("set_enabled", { enabled: next });
    } catch (err) {
      setEnabled(!next);
      setToggleError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleSyncNow() {
    setSyncing(true);
    setSyncError(null);
    try {
      const result = await invoke<SyncResult>("sync_now");
      setSyncResult(result);
      setLastSynced(new Date());
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err));
    } finally {
      setSyncing(false);
    }
  }

  async function handlePickFolder() {
    const picked = await invoke<string | null>("pick_run_folder");
    if (picked !== null) {
      setRunRoot(picked);
      onConfigChange();
    }
  }

  async function handleSetFolder(value: string) {
    const newRoot = value.trim() || null;
    await invoke("set_config", {
      config: { ...config, run_root: newRoot },
    });
    setRunRoot(newRoot);
    onConfigChange();
  }

  async function handleToggleAutostart(next: boolean) {
    setAutostart(next);
    setToggleError(null);
    try {
      await invoke("set_autostart", { enabled: next });
    } catch (err) {
      setAutostart(!next);
      setToggleError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleUnpair() {
    await invoke("unpair");
    onUnpaired();
  }

  async function handleOpenFolder() {
    await invoke("open_run_folder");
  }

  function formatLastSynced(date: Date): string {
    const diffMs = Date.now() - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 60) return "just now";
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin} min ago`;
    const diffHr = Math.floor(diffMin / 60);
    return `${diffHr} hr ago`;
  }

  function fileBasename(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  return (
    <div className="min-h-screen bg-zinc-950 px-5 py-5 text-zinc-100">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-zinc-100 text-lg font-semibold">STS2 Sync</h1>
        <button
          onClick={() => setShowSettings((s) => !s)}
          title="Settings"
          className={`w-8 h-8 rounded-lg flex items-center justify-center transition-colors ${
            showSettings
              ? "bg-zinc-700 text-zinc-100"
              : "text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
          }`}
        >
          <GearIcon />
        </button>
      </div>

      {showSettings ? (
        <SettingsPanel
          runRoot={runRoot}
          autostart={autostart}
          onPickFolder={handlePickFolder}
          onSetFolder={handleSetFolder}
          onToggleAutostart={handleToggleAutostart}
          onUnpair={handleUnpair}
        />
      ) : (
        <>
          {/* Connection status */}
          <div className="bg-zinc-900 rounded-xl p-4 mb-4">
            <div className="flex items-center gap-2 mb-1">
              <span className="w-2 h-2 rounded-full bg-green-400 inline-block" />
              <span className="text-zinc-100 text-sm font-medium">
                Connected to {dashboardHost}
              </span>
            </div>
            <p className="text-zinc-500 text-xs pl-4">
              {lastSynced
                ? `Last synced: ${formatLastSynced(lastSynced)}`
                : "Not yet synced this session"}
            </p>
          </div>

          {/* Sync toggle */}
          <div className="flex items-center justify-between bg-zinc-900 rounded-xl px-4 py-3 mb-4">
            <span className="text-zinc-200 text-sm">Syncing</span>
            <Toggle value={enabled} onChange={handleToggleEnabled} />
          </div>

          {toggleError && (
            <p className="mb-4 text-red-400 text-sm bg-red-950/40 border border-red-900/50 rounded-lg px-3 py-2">
              {toggleError}
            </p>
          )}

          {/* Sync Now */}
          <button
            onClick={handleSyncNow}
            disabled={syncing}
            className="w-full bg-indigo-600 hover:bg-indigo-500 active:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium py-2.5 px-4 rounded-lg transition-colors mb-4"
          >
            {syncing ? "Syncing…" : "Sync Now"}
          </button>

          {syncError && (
            <p className="mb-4 text-red-400 text-sm bg-red-950/40 border border-red-900/50 rounded-lg px-3 py-2">
              {syncError}
            </p>
          )}

          {/* Sync result */}
          {syncResult && (
            <div className="bg-zinc-900 rounded-xl p-4 mb-4">
              <div className="flex gap-4 mb-3">
                <Stat label="Uploaded" value={syncResult.imported} color="text-green-400" />
                <Stat label="Duplicate" value={syncResult.duplicates} color="text-zinc-500" />
                <Stat label="Errors" value={syncResult.errors} color="text-red-400" />
              </div>
              {syncResult.files.length > 0 && (
                <div className="space-y-1">
                  <p className="text-zinc-500 text-xs mb-2">Recent runs (last 5):</p>
                  {syncResult.files.slice(0, 5).map((f, i) => (
                    <FileRow key={i} file={f} basename={fileBasename(f.path)} />
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Privacy notice */}
          <div className="border-t border-zinc-800 pt-4 mb-4">
            <p className="text-zinc-500 text-xs leading-relaxed mb-3">
              This app only reads <code className="text-zinc-400">*.run</code> files
              in your STS2 save folder. Nothing else leaves your PC.
            </p>
            <div className="flex gap-2">
              <button
                onClick={handleOpenFolder}
                className="text-xs text-zinc-400 hover:text-zinc-200 bg-zinc-900 hover:bg-zinc-800 px-3 py-1.5 rounded-lg transition-colors"
              >
                Open save folder
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// ── Settings panel ────────────────────────────────────────────────────────────

function SettingsPanel({
  runRoot,
  autostart,
  onPickFolder,
  onSetFolder,
  onToggleAutostart,
  onUnpair,
}: {
  runRoot: string | null;
  autostart: boolean;
  onPickFolder: () => void;
  onSetFolder: (value: string) => Promise<void>;
  onToggleAutostart: (v: boolean) => void;
  onUnpair: () => void;
}) {
  const [folderInput, setFolderInput] = useState(runRoot ?? "");
  const [folderSaving, setFolderSaving] = useState(false);

  async function handleSetFolder() {
    setFolderSaving(true);
    try {
      await onSetFolder(folderInput);
    } finally {
      setFolderSaving(false);
    }
  }

  return (
    <div className="space-y-4">
      {/* Save folder */}
      <div className="bg-zinc-900 rounded-xl p-4">
        <p className="text-zinc-400 text-xs mb-2">Save folder</p>
        <p className="text-zinc-500 text-xs mb-2">
          Leave empty to use the default save folder.
        </p>
        <input
          type="text"
          value={folderInput}
          onChange={(e) => setFolderInput(e.target.value)}
          placeholder={`e.g. C:\\Users\\you\\AppData\\Roaming\\SlayTheSpire2\\steam`}
          className="w-full bg-zinc-800 border border-zinc-700 focus:border-indigo-500 text-zinc-100 text-xs rounded-lg px-3 py-2 outline-none transition-colors placeholder:text-zinc-600 mb-3"
        />
        <div className="flex gap-2">
          <button
            onClick={() => void handleSetFolder()}
            disabled={folderSaving}
            className="text-xs text-zinc-300 hover:text-zinc-100 disabled:opacity-40 bg-zinc-800 hover:bg-zinc-700 px-3 py-1.5 rounded-lg transition-colors"
          >
            {folderSaving ? "Saving…" : "Set folder"}
          </button>
          <button
            onClick={onPickFolder}
            className="text-xs text-zinc-500 hover:text-zinc-300 bg-zinc-800 hover:bg-zinc-700 px-3 py-1.5 rounded-lg transition-colors"
          >
            Browse…
          </button>
        </div>
      </div>

      {/* Launch on login */}
      <div className="bg-zinc-900 rounded-xl px-4 py-3 flex items-center justify-between">
        <span className="text-zinc-200 text-sm">Launch on login</span>
        <Toggle value={autostart} onChange={onToggleAutostart} />
      </div>

      {/* Unpair */}
      <div className="bg-zinc-900 rounded-xl p-4">
        <p className="text-zinc-400 text-xs mb-1">Device</p>
        <p className="text-zinc-500 text-xs mb-3">
          Unpair this device from the dashboard.
        </p>
        <button
          onClick={onUnpair}
          className="text-xs text-red-400 hover:text-red-300 bg-red-950/30 hover:bg-red-950/50 border border-red-900/40 px-3 py-1.5 rounded-lg transition-colors"
        >
          Unpair device
        </button>
      </div>
    </div>
  );
}

// ── Small components ──────────────────────────────────────────────────────────

function Toggle({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={value}
      onClick={() => onChange(!value)}
      className={`relative w-10 h-6 rounded-full transition-colors focus:outline-none ${
        value ? "bg-indigo-600" : "bg-zinc-700"
      }`}
    >
      <span
        className={`absolute top-1 left-1 w-4 h-4 rounded-full bg-white shadow transition-transform ${
          value ? "translate-x-4" : "translate-x-0"
        }`}
      />
    </button>
  );
}

function Stat({
  label,
  value,
  color,
}: {
  label: string;
  value: number;
  color: string;
}) {
  return (
    <div className="text-center">
      <p className={`text-lg font-semibold ${color}`}>{value}</p>
      <p className="text-zinc-500 text-xs">{label}</p>
    </div>
  );
}

function FileRow({
  file,
  basename,
}: {
  file: RunFileStatus;
  basename: string;
}) {
  const icon =
    file.status === "uploaded"
      ? "✓"
      : file.status === "duplicate"
      ? "↺"
      : "✗";
  const color =
    file.status === "uploaded"
      ? "text-green-400"
      : file.status === "duplicate"
      ? "text-zinc-500"
      : "text-red-400";

  return (
    <div className="flex items-start gap-2 text-xs">
      <span className={`font-mono mt-0.5 ${color}`}>{icon}</span>
      <div className="flex-1 min-w-0">
        <span className="text-zinc-300 truncate block">{basename}</span>
        {file.status === "error" && file.error && (
          <span className="text-red-400 text-xs">{file.error}</span>
        )}
      </div>
      <span className={`shrink-0 ${color}`}>{file.status}</span>
    </div>
  );
}

function GearIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}
