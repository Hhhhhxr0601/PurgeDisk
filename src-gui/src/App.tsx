import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

// ============================================================
// Types (merged to reduce file count)
// ============================================================

interface Device {
  path: string;
  volume_paths: string[];
  model: string;
  serial: string;
  size_bytes: number;
  removable: boolean;
  bus_type: string;
  is_system: boolean;
}

interface LogEntry {
  timestamp: string;
  operator: string;
  hostname: string;
  device_serial: string;
  device_path: string;
  device_size_bytes: number;
  algorithm: string;
  pass_number: number;
  pass_pattern: string;
  status: string;
  error_detail: string | null;
  hmac_signature: string;
}

interface WipeEventProgress {
  event: "Progress";
  data: {
    pass: number;
    total_passes: number;
    percent: number;
    written_bytes: number;
    total_bytes: number;
    speed_bps: number;
    eta_seconds: number;
  };
}

interface WipeEventPassComplete {
  event: "PassComplete";
  data: {
    pass: number;
    pattern: string;
    bytes_written: number;
    elapsed_seconds: number;
    success: boolean;
  };
}

interface WipeEventFinished {
  event: "Finished";
  data: {
    success: boolean;
    total_written: number;
    total_size: number;
    elapsed_seconds: number;
    error: string | null;
  };
}

interface WipeEventError {
  event: "Error";
  data: { message: string };
}

type WipeEvent =
  | WipeEventProgress
  | WipeEventPassComplete
  | WipeEventFinished
  | WipeEventError;

// ============================================================
// Helpers
// ============================================================

function formatBytes(bytes: number): string {
  if (bytes >= 1_099_511_627_776) return `${(bytes / 1_099_511_627_776).toFixed(2)} TB`;
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatDuration(seconds: number): string {
  const s = Math.floor(seconds);
  if (s >= 3600) return `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m${s % 60}s`;
  if (s >= 60) return `${Math.floor(s / 60)}m${s % 60}s`;
  return `${s}s`;
}

// ============================================================
// Pages
// ============================================================

type Page = "devices" | "confirm" | "progress" | "result" | "logs";

// --- Device List Page ---
function DeviceListPage({
  devices,
  loading,
  error,
  onSelect,
  onRefresh,
}: {
  devices: Device[];
  loading: boolean;
  error: string | null;
  onSelect: (d: Device) => void;
  onRefresh: () => void;
}) {
  return (
    <div className="page">
      <div className="page-header">
        <h2>⚡ 可移动存储设备</h2>
        <button onClick={onRefresh} className="btn btn-secondary">刷新</button>
      </div>
      {loading && <div className="status-info">正在扫描可移动存储设备...</div>}
      {error && <div className="status-error">扫描失败: {error}</div>}
      {!loading && !error && devices.length === 0 && (
        <div className="status-info">未检测到可移动存储设备。请确认 U 盘已插入并被系统识别。</div>
      )}
      {!loading && devices.length > 0 && (
        <table className="device-table">
          <thead>
            <tr>
              <th>#</th>
              <th>设备型号</th>
              <th>路径</th>
              <th>序列号</th>
              <th>容量</th>
              <th>总线</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {devices.map((d, i) => (
              <tr key={d.path}>
                <td>{i + 1}</td>
                <td>{d.model}</td>
                <td className="mono">{d.path}</td>
                <td className="mono">{d.serial}</td>
                <td>{formatBytes(d.size_bytes)}</td>
                <td>{d.bus_type}</td>
                <td>
                  <button onClick={() => onSelect(d)} className="btn btn-primary">擦写</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

// --- Confirm Page ---
function ConfirmPage({
  device,
  onConfirm,
  onCancel,
}: {
  device: Device;
  onConfirm: (serial: string) => void;
  onCancel: () => void;
}) {
  const [input, setInput] = useState("");
  const [error, setError] = useState("");

  const handleConfirm = () => {
    if (input.trim() === device.serial) {
      onConfirm(input.trim());
    } else {
      setError("序列号不匹配，请重新输入。");
    }
  };

  return (
    <div className="page">
      <div className="page-header">
        <h2>⚠ 确认擦写操作</h2>
      </div>
      <div className="warning-box">
        <strong>警告：即将销毁设备上的所有数据，此操作不可逆！</strong>
      </div>
      <div className="device-info-card">
        <div className="info-row"><span className="label">设备型号:</span> <span className="value">{device.model}</span></div>
        <div className="info-row"><span className="label">设备路径:</span> <span className="value mono">{device.path}</span></div>
        <div className="info-row"><span className="label">序 列 号:</span> <span className="value highlight">{device.serial}</span></div>
        <div className="info-row"><span className="label">容    量:</span> <span className="value">{formatBytes(device.size_bytes)}</span></div>
        <div className="info-row"><span className="label">总    线:</span> <span className="value">{device.bus_type}</span></div>
        <div className="info-row"><span className="label">算    法:</span> <span className="value">DoD 5220.22-M (三轮覆写: 0x00 → 0xFF → 随机)</span></div>
      </div>
      <div className="serial-input-section">
        <label>请输入设备序列号以确认操作:</label>
        <div className="input-row">
          <input
            type="text"
            value={input}
            onChange={(e) => { setInput(e.target.value); setError(""); }}
            placeholder={device.serial}
            className={error ? "input-error" : ""}
          />
          <span className="hint">预期: {device.serial}</span>
        </div>
        {error && <div className="error-text">{error}</div>}
      </div>
      <div className="button-row">
        <button onClick={handleConfirm} className="btn btn-danger">确认擦写</button>
        <button onClick={onCancel} className="btn btn-secondary">取消</button>
      </div>
    </div>
  );
}

// --- Progress Page ---
function ProgressPage({
  events,
  onCancel,
}: {
  events: WipeEvent[];
  onCancel: () => void;
}) {
  const progress = events.filter((e) => e.event === "Progress").pop() as WipeEventProgress | undefined;
  const finished = events.find((e) => e.event === "Finished") as WipeEventFinished | undefined;
  const errorEvent = events.find((e) => e.event === "Error") as WipeEventError | undefined;

  if (finished) {
    return null; // Will be handled by parent
  }

  const p = progress?.data;
  const passLabels = ["0x00 填充", "0xFF 填充", "随机数据"];

  return (
    <div className="page">
      <div className="page-header">
        <h2>🔄 正在执行 DoD 5220.22-M 三轮覆写...</h2>
      </div>
      {[0, 1, 2].map((i) => {
        const passData = p && p.pass === i + 1;
        const passDone = p && p.pass > i + 1;
        const percent = passDone ? 100 : passData ? p!.percent : 0;
        return (
          <div key={i} className="pass-bar">
            <div className="pass-label">
              {passDone ? "✓" : passData ? "▶" : "○"} 第 {i + 1} 轮: {passLabels[i]}
            </div>
            <div className="progress-track">
              <div
                className={`progress-fill ${passDone ? "done" : passData ? "active" : ""}`}
                style={{ width: `${percent}%` }}
              />
            </div>
            <span className="pass-percent">{percent.toFixed(1)}%</span>
          </div>
        );
      })}
      {p && (
        <div className="stats-grid">
          <div className="stat"><span className="stat-label">速度:</span> <span className="stat-value green">{formatBytes(p.speed_bps)}/s</span></div>
          <div className="stat"><span className="stat-label">已写入:</span> <span className="stat-value">{formatBytes(p.written_bytes)} / {formatBytes(p.total_bytes)}</span></div>
          <div className="stat"><span className="stat-label">总体进度:</span> <span className="stat-value">{p.percent.toFixed(1)}%</span></div>
          <div className="stat"><span className="stat-label">预计剩余:</span> <span className="stat-value">{formatDuration(p.eta_seconds)}</span></div>
        </div>
      )}
      {errorEvent && <div className="status-error">{errorEvent.data.message}</div>}
      <div className="button-row">
        <button onClick={onCancel} className="btn btn-danger">取消擦写</button>
      </div>
    </div>
  );
}

// --- Result Page ---
function ResultPage({
  finished,
  error,
  onBack,
}: {
  finished: WipeEventFinished | null;
  error: string | null;
  onBack: () => void;
}) {
  return (
    <div className="page">
      <div className="page-header">
        <h2>{finished?.data.success ? "✓ 擦写完成" : error ? "✗ 操作失败" : "✗ 擦写未完成"}</h2>
      </div>
      {error && <div className="status-error">{error}</div>}
      {finished && (
        <div className="result-card">
          <div className="info-row"><span className="label">总写入:</span> <span className="value">{formatBytes(finished.data.total_written)}</span></div>
          <div className="info-row"><span className="label">设备容量:</span> <span className="value">{formatBytes(finished.data.total_size)}</span></div>
          <div className="info-row"><span className="label">总耗时:</span> <span className="value">{formatDuration(finished.data.elapsed_seconds)}</span></div>
          <div className="info-row"><span className="label">平均速度:</span> <span className="value green">
            {finished.data.elapsed_seconds > 0
              ? `${formatBytes(finished.data.total_written / finished.data.elapsed_seconds)}/s`
              : "N/A"}
          </span></div>
          {finished.data.error && <div className="info-row"><span className="label">错误信息:</span> <span className="value error">{finished.data.error}</span></div>}
        </div>
      )}
      <div className="button-row">
        <button onClick={onBack} className="btn btn-primary">返回设备列表</button>
      </div>
    </div>
  );
}

// --- Logs Page ---
function LogsPage({ logs, loading }: { logs: LogEntry[]; loading: boolean }) {
  const statusColor = (status: string) => {
    if (status === "completed") return "green";
    if (status === "error" || status === "interrupted") return "red";
    if (status === "cancelled") return "yellow";
    return "";
  };

  return (
    <div className="page">
      <div className="page-header">
        <h2>📋 审计日志 ({logs.length} 条)</h2>
      </div>
      {loading && <div className="status-info">正在加载审计日志...</div>}
      {!loading && logs.length === 0 && <div className="status-info">暂无审计日志记录。</div>}
      {logs.length > 0 && (
        <div className="log-table-wrap">
          <table className="log-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>操作人</th>
                <th>设备序列号</th>
                <th>算法</th>
                <th>状态</th>
                <th>详情</th>
              </tr>
            </thead>
            <tbody>
              {logs.map((entry, i) => (
                <tr key={i}>
                  <td className="mono">{entry.timestamp.slice(0, 19)}</td>
                  <td>{entry.operator}</td>
                  <td className="mono">{entry.device_serial}</td>
                  <td>{entry.algorithm}</td>
                  <td><span className={`status-badge ${statusColor(entry.status)}`}>{entry.status}</span></td>
                  <td>{entry.error_detail || ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ============================================================
// Main App
// ============================================================

export default function App() {
  const [page, setPage] = useState<Page>("devices");
  const [devices, setDevices] = useState<Device[]>([]);
  const [devicesLoading, setDevicesLoading] = useState(true);
  const [devicesError, setDevicesError] = useState<string | null>(null);
  const [selectedDevice, setSelectedDevice] = useState<Device | null>(null);
  const [wipeEvents, setWipeEvents] = useState<WipeEvent[]>([]);
  const [wipeFinished, setWipeFinished] = useState<WipeEventFinished | null>(null);
  const [wipeError, setWipeError] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [logsLoading, setLogsLoading] = useState(false);

  // Scan devices
  const scanDevices = async () => {
    setDevicesLoading(true);
    setDevicesError(null);
    try {
      const result = await invoke<Device[]>("list_devices");
      setDevices(result);
    } catch (e) {
      setDevicesError(String(e));
    } finally {
      setDevicesLoading(false);
    }
  };

  useEffect(() => {
    scanDevices();
  }, []);

  // Load logs
  const loadLogs = async () => {
    setLogsLoading(true);
    try {
      const result = await invoke<LogEntry[]>("get_logs");
      setLogs(result);
    } catch (e) {
      console.error("Failed to load logs:", e);
    } finally {
      setLogsLoading(false);
    }
  };

  // Listen for wipe events from Rust
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<WipeEvent>("wipe-event", (event) => {
        const wipeEvent = event.payload;
        if (wipeEvent.event === "Finished") {
          setWipeFinished(wipeEvent as WipeEventFinished);
          setPage("result");
        } else if (wipeEvent.event === "Error") {
          setWipeError((wipeEvent as WipeEventError).data.message);
          setPage("result");
        } else {
          setWipeEvents((prev) => [...prev, wipeEvent]);
        }
      }).then((unlistenFn) => {
        unlisten = unlistenFn;
      });
    });
    return () => { unlisten?.(); };
  }, []);

  // Start wipe
  const handleStartWipe = async (_serial: string) => {
    if (!selectedDevice) return;
    setWipeEvents([]);
    setWipeFinished(null);
    setWipeError(null);
    setPage("progress");
    try {
      await invoke("start_wipe", {
        devicePath: selectedDevice.path,
        deviceSerial: selectedDevice.serial,
        deviceSize: selectedDevice.size_bytes,
      });
    } catch (e) {
      setWipeError(String(e));
      setPage("result");
    }
  };

  // Navigation
  const goDevices = () => {
    setPage("devices");
    scanDevices();
  };

  const goLogs = () => {
    setPage("logs");
    loadLogs();
  };

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="sidebar-title">PurgeDisk</div>
        <button
          className={`nav-btn ${page === "devices" || page === "confirm" || page === "progress" || page === "result" ? "active" : ""}`}
          onClick={goDevices}
        >
          🖥 设备列表
        </button>
        <button
          className={`nav-btn ${page === "logs" ? "active" : ""}`}
          onClick={goLogs}
        >
          📋 审计日志
        </button>
      </nav>
      <main className="content">
        {page === "devices" && (
          <DeviceListPage
            devices={devices}
            loading={devicesLoading}
            error={devicesError}
            onSelect={(d) => { setSelectedDevice(d); setPage("confirm"); }}
            onRefresh={scanDevices}
          />
        )}
        {page === "confirm" && selectedDevice && (
          <ConfirmPage
            device={selectedDevice}
            onConfirm={handleStartWipe}
            onCancel={goDevices}
          />
        )}
        {page === "progress" && (
          <ProgressPage events={wipeEvents} onCancel={goDevices} />
        )}
        {page === "result" && (
          <ResultPage finished={wipeFinished} error={wipeError} onBack={goDevices} />
        )}
        {page === "logs" && <LogsPage logs={logs} loading={logsLoading} />}
      </main>
    </div>
  );
}
