import { useMemo, useState, type ReactNode } from "react";
import { Button, Input, Label, Modal, Spinner, TextField, toast } from "@heroui/react";
import { ArrowDown, ArrowUp, ExternalLink, Pencil, RefreshCw } from "lucide-react";
import {
  commands,
  type AmpAssignment,
  type AmpEditLock,
  type AmpModelCatalogEntry,
  type ChannelConfigSnapshot,
  type DiscoveredDevice,
  type Project,
  type Telemetry,
} from "../lib/bindings";
import { ampLinkStatus, linkedDeviceFor } from "../lib/ampLinkStatus";
import { StandbyToggle } from "./StandbyToggle";

const SECTION_TITLE =
  "text-[length:var(--amp-font-size-xs)] font-semibold uppercase tracking-widest text-[var(--amp-color-dimmed)]";

const STATUS_LABEL = { online: "Online", offline: "Offline", unlinked: "Virtual" } as const;
const STATUS_COLOR = {
  online: "var(--amp-color-green-6)",
  offline: "var(--amp-color-red-6)",
  unlinked: "var(--amp-color-dimmed)",
} as const;

/** One row of the list: a project amp with whatever its linked hardware
 * currently reports. */
interface Row {
  assignment: AmpAssignment;
  device: DiscoveredDevice | undefined;
  status: keyof typeof STATUS_LABEL;
  name: string;
  model: string;
  telemetry: Telemetry | undefined;
  config: ChannelConfigSnapshot | undefined;
}

type ColumnKey = "status" | "name" | "model" | "temp" | "firmware" | "ip" | "mac";

interface Column {
  key: ColumnKey;
  label: string;
  width: number;
  /** Text the filter and the sort compare against. */
  text: (row: Row) => string;
  /** Numeric sort, where text would order "9" after "10". */
  num?: (row: Row) => number | null;
  render?: (row: Row) => ReactNode;
}

function maxTemp(t: Telemetry | undefined): number | null {
  const temps = (t?.temperatures ?? []).filter((v): v is number => v !== null);
  return temps.length > 0 ? Math.max(...temps) : null;
}

const COLUMNS: Column[] = [
  {
    key: "status",
    label: "Status",
    width: 90,
    text: (r) => STATUS_LABEL[r.status],
    render: (r) => <span style={{ color: STATUS_COLOR[r.status] }}>{STATUS_LABEL[r.status]}</span>,
  },
  { key: "name", label: "Name", width: 200, text: (r) => r.name, render: (r) => <span className="font-medium">{r.name}</span> },
  { key: "model", label: "Model", width: 140, text: (r) => r.model },
  {
    key: "temp",
    label: "Temp",
    width: 80,
    text: (r) => {
      const t = maxTemp(r.telemetry);
      return t === null ? "" : `${t.toFixed(0)} °C`;
    },
    num: (r) => maxTemp(r.telemetry),
  },
  { key: "firmware", label: "Firmware", width: 120, text: (r) => r.device?.firmwareVersion ?? r.assignment.firmwareVersion ?? "" },
  { key: "ip", label: "IP", width: 130, text: (r) => r.device?.ip ?? "" },
  { key: "mac", label: "MAC Address", width: 150, text: (r) => r.device?.mac ?? r.assignment.mac ?? "" },
];

interface DeviceListViewProps {
  project: Project;
  onProjectUpdate: (project: Project) => void;
  ampModels: AmpModelCatalogEntry[] | null;
  devices: DiscoveredDevice[];
  telemetryById: Record<string, Telemetry>;
  channelConfigById: Record<string, ChannelConfigSnapshot>;
  locks: Map<string, AmpEditLock>;
  onOpenDevice: (assignment: AmpAssignment) => void;
}

/** Every amp of the project as one table — status, standby, hard mute and the
 * linked hardware's identity at a glance — with the selected amp's details on
 * the right, like the vendor's device list. */
export function DeviceListView({
  project,
  onProjectUpdate,
  ampModels,
  devices,
  telemetryById,
  channelConfigById,
  locks,
  onOpenDevice,
}: DeviceListViewProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filters, setFilters] = useState<Partial<Record<ColumnKey, string>>>({});
  const [sort, setSort] = useState<{ key: ColumnKey; desc: boolean } | null>(null);
  const [renameTarget, setRenameTarget] = useState<AmpAssignment | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [renameError, setRenameError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const rows = useMemo<Row[]>(() => {
    const modelsById = new Map((ampModels ?? []).map((m) => [m.id, m]));
    return project.ampAssignments.map((assignment) => {
      const device = linkedDeviceFor(assignment, devices);
      const model = assignment.ampModelId ? modelsById.get(assignment.ampModelId) : undefined;
      const modelName = model ? `${model.brand} ${model.model}` : "";
      return {
        assignment,
        device,
        status: ampLinkStatus(assignment, devices),
        name: assignment.deviceName ?? device?.name ?? (modelName || "Unnamed"),
        model: modelName,
        telemetry: device ? telemetryById[device.id] : undefined,
        config: device ? channelConfigById[device.id] : undefined,
      };
    });
  }, [project.ampAssignments, ampModels, devices, telemetryById, channelConfigById]);

  const visibleRows = useMemo(() => {
    let out = rows.filter((row) =>
      COLUMNS.every((col) => {
        const f = filters[col.key]?.trim().toLowerCase();
        return !f || col.text(row).toLowerCase().includes(f);
      }),
    );
    if (sort) {
      const col = COLUMNS.find((c) => c.key === sort.key)!;
      out = [...out].sort((a, b) => {
        let cmp: number;
        if (col.num) {
          // Unknown values last, whichever the direction.
          const av = col.num(a);
          const bv = col.num(b);
          if (av === null || bv === null) return av === bv ? 0 : av === null ? 1 : -1;
          cmp = av - bv;
        } else {
          cmp = col.text(a).localeCompare(col.text(b), undefined, { numeric: true });
        }
        return sort.desc ? -cmp : cmp;
      });
    }
    return out;
  }, [rows, filters, sort]);

  const selected = rows.find((r) => r.assignment.id === selectedId) ?? null;

  function toggleSort(key: ColumnKey) {
    setSort((prev) => (prev?.key !== key ? { key, desc: false } : prev.desc ? null : { key, desc: true }));
  }

  async function toggleMute(row: Row, channelIndex: number, muted: boolean) {
    // Through the project, so a linked amp the project follows is written too.
    const result = await commands.projectsSetChannelOutputMute(project.id, row.assignment.id, channelIndex, muted);
    if (result.status === "ok") onProjectUpdate(result.data);
    else toast.danger("Mute failed", { description: result.error.message });
  }

  async function refreshAll() {
    setRefreshing(true);
    await Promise.all(
      rows.flatMap((r) => (r.device?.online ? [commands.liveControlRefreshNow(r.device.id)] : [])),
    );
    setRefreshing(false);
  }

  function openRename(assignment: AmpAssignment) {
    setRenameError(null);
    setRenameValue(assignment.deviceName ?? "");
    setRenameTarget(assignment);
  }

  async function saveRename() {
    if (!renameTarget) return;
    setRenaming(true);
    setRenameError(null);
    const result = await commands.projectsEditAmp(
      project.id,
      renameTarget.id,
      renameValue.trim() || null,
      renameTarget.firmwareVersion ?? null,
    );
    setRenaming(false);
    if (result.status === "ok") {
      onProjectUpdate(result.data);
      setRenameTarget(null);
    } else {
      setRenameError(result.error.message);
    }
  }

  const gridTemplate = `${COLUMNS[0].width}px 56px ${COLUMNS.slice(1, 3)
    .map((c) => `${c.width}px`)
    .join(" ")} minmax(160px, 1fr) ${COLUMNS.slice(3)
    .map((c) => `${c.width}px`)
    .join(" ")}`;

  // Status, PWR, Name, Model, Hard Mute, then the rest — PWR and Hard Mute
  // are controls rather than data, so they take neither filter nor sort.
  const headerCells: (Column | "pwr" | "mute")[] = [COLUMNS[0], "pwr", COLUMNS[1], COLUMNS[2], "mute", ...COLUMNS.slice(3)];
  const cellBorder = "border-r border-[var(--amp-color-default-border)]";

  return (
    <div className="flex h-full min-h-0 gap-2 p-2">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-[var(--amp-color-default-border)] bg-[var(--amp-color-body)]">
        <div className="flex items-center gap-3 px-3 py-2">
          <div className="flex items-baseline gap-1.5">
            <span className="text-2xl font-light">{rows.length}</span>
            <span className="text-base">Amplifiers</span>
          </div>
          <div className="flex-1" />
          <span className="text-[length:var(--amp-font-size-sm)] text-[var(--amp-color-dimmed)]">
            {selected ? "1 unit selected" : "No unit selected"}
          </span>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label="Refresh"
            onPress={refreshAll}
            isDisabled={refreshing || !rows.some((r) => r.device?.online)}
          >
            {refreshing ? <Spinner size="sm" /> : <RefreshCw size={14} />}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          <div className="min-w-max" style={{ display: "grid", gridTemplateColumns: gridTemplate }}>
            {headerCells.map((cell) => {
              if (cell === "pwr" || cell === "mute") {
                return (
                  <div
                    key={cell}
                    className={`sticky top-0 z-10 bg-[var(--amp-color-body)] px-2 py-1.5 ${SECTION_TITLE} ${cellBorder}`}
                  >
                    {cell === "pwr" ? "Pwr" : "Hard Mute"}
                  </div>
                );
              }
              const active = sort?.key === cell.key;
              return (
                <button
                  key={cell.key}
                  type="button"
                  onClick={() => toggleSort(cell.key)}
                  className={`sticky top-0 z-10 flex appearance-none items-center justify-between gap-1 bg-[var(--amp-color-body)] px-2 py-1.5 text-left font-inherit ${SECTION_TITLE} ${cellBorder}`}
                >
                  {cell.label}
                  {active && (sort.desc ? <ArrowDown size={12} /> : <ArrowUp size={12} />)}
                </button>
              );
            })}

            {headerCells.map((cell) =>
              typeof cell === "string" ? (
                <div key={`f-${cell}`} className={`bg-[var(--amp-color-default)] ${cellBorder}`} />
              ) : (
                <input
                  key={`f-${cell.key}`}
                  value={filters[cell.key] ?? ""}
                  onChange={(e) => setFilters((prev) => ({ ...prev, [cell.key]: e.target.value }))}
                  placeholder="Filter"
                  className={`min-w-0 bg-[var(--amp-color-default)] px-2 py-1 text-[length:var(--amp-font-size-sm)] italic outline-none placeholder:text-[var(--amp-color-dimmed)] ${cellBorder}`}
                />
              ),
            )}

            {visibleRows.map((row) => {
              const isSelected = row.assignment.id === selectedId;
              const lock = locks.get(row.assignment.id);
              const rowClass = `flex min-h-12 items-center px-2 text-[length:var(--amp-font-size-sm)] ${cellBorder} border-b ${
                isSelected ? "bg-[var(--accent-soft)]" : ""
              }`;
              const select = () => setSelectedId(row.assignment.id);
              const cells: ReactNode[] = headerCells.map((cell, i) => {
                const first = i === 0 ? "border-l-4 border-l-[var(--accent)]" : "";
                if (cell === "pwr") {
                  return (
                    <div key="pwr" className={`${rowClass} justify-center`} onClick={select}>
                      {row.device?.online && !row.assignment.liveDisengaged ? (
                        <StandbyToggle
                          deviceId={row.device.id}
                          standby={row.config?.standby ?? lock?.standby}
                          standbyLocked={row.config?.standbyLocked ?? lock?.standbyLocked}
                        />
                      ) : null}
                    </div>
                  );
                }
                if (cell === "mute") {
                  return (
                    <div key="mute" className={`${rowClass} gap-1`} onClick={select}>
                      {row.assignment.channels.map((ch) => {
                        const muted = ch.outputMuted ?? false;
                        return (
                          <button
                            key={ch.channelIndex}
                            type="button"
                            title={muted ? "Muted — click to unmute" : "Unmuted — click to mute"}
                            onClick={(e) => {
                              e.stopPropagation();
                              void toggleMute(row, ch.channelIndex, !muted);
                            }}
                            className="flex h-5 w-5 appearance-none items-center justify-center rounded-sm text-[11px] font-semibold text-white"
                            style={{ background: muted ? "var(--amp-color-red-8, #a51d1d)" : "var(--amp-color-gray-7, #4a4a4a)" }}
                          >
                            {ch.channelIndex + 1}
                          </button>
                        );
                      })}
                    </div>
                  );
                }
                return (
                  <div
                    key={cell.key}
                    className={`${rowClass} ${first} truncate`}
                    onClick={select}
                    onDoubleClick={() => onOpenDevice(row.assignment)}
                  >
                    {cell.render ? cell.render(row) : cell.text(row) || <span className="text-[var(--amp-color-dimmed)]">—</span>}
                  </div>
                );
              });
              return <div key={row.assignment.id} className="contents cursor-default">{cells}</div>;
            })}
          </div>
          {rows.length === 0 && (
            <div className="p-6 text-center text-[var(--amp-color-dimmed)]">
              No amplifiers in this project yet — add them in the Workspace.
            </div>
          )}
        </div>
      </div>

      <DetailsPanel
        row={selected}
        onOpen={() => selected && onOpenDevice(selected.assignment)}
        onRename={() => selected && openRename(selected.assignment)}
      />

      <Modal.Backdrop isOpen={renameTarget !== null} onOpenChange={(open) => !open && setRenameTarget(null)}>
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>Rename Amp</Modal.Heading>
              <Modal.CloseTrigger />
            </Modal.Header>
            <Modal.Body>
              <div className="flex flex-col gap-3">
                <TextField autoFocus>
                  <Label>Device Name</Label>
                  <Input
                    placeholder="Optional"
                    maxLength={32}
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && void saveRename()}
                  />
                </TextField>
                {renameError && (
                  <span style={{ color: "var(--amp-color-red-6)", fontSize: "var(--amp-font-size-sm)" }}>
                    {renameError}
                  </span>
                )}
                <div className="flex justify-end gap-2">
                  <Button variant="secondary" onPress={() => setRenameTarget(null)} isDisabled={renaming}>
                    Cancel
                  </Button>
                  <Button variant="primary" onPress={saveRename} isDisabled={renaming}>
                    {renaming ? <Spinner size="sm" /> : "Save"}
                  </Button>
                </div>
              </div>
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  );
}

function DetailsPanel({ row, onOpen, onRename }: { row: Row | null; onOpen: () => void; onRename: () => void }) {
  const fields: [string, ReactNode][] = row
    ? [
        ["Name", row.name],
        ["Status", <span style={{ color: STATUS_COLOR[row.status] }}>{STATUS_LABEL[row.status]}</span>],
        ["Model", row.model || "—"],
        [
          "Channel",
          row.device
            ? `${row.device.analogInputChannels}IN-${row.device.outputChannels}OUT`
            : `${row.assignment.channels.length} OUT`,
        ],
        ["MAC", row.device?.mac ?? row.assignment.mac ?? "—"],
        ["IP", row.device?.ip ?? "—"],
        ["Firmware", row.device?.firmwareVersion ?? row.assignment.firmwareVersion ?? "—"],
        [
          "Temperature",
          row.telemetry
            ? row.telemetry.temperatures
                .map((t, i) => (t === null ? null : `${i < 4 ? `CH${i + 1}` : "PSU"} ${t.toFixed(0)} °C`))
                .filter(Boolean)
                .join(" · ") || "—"
            : "—",
        ],
        ["Last Seen", row.device?.lastSeenAt ? new Date(row.device.lastSeenAt).toLocaleTimeString() : "—"],
        ["Identifier", <span className="font-mono text-[11px]">{row.assignment.id}</span>],
      ]
    : [];

  return (
    <div className="flex w-72 shrink-0 flex-col rounded-md border border-[var(--amp-color-default-border)] bg-[var(--amp-color-body)]">
      <div className={`px-3 py-2.5 ${SECTION_TITLE}`}>Details</div>
      {row ? (
        <>
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-3 pb-3">
            {fields.map(([label, value]) => (
              <div key={label} className="flex flex-col gap-0.5">
                <span className="text-[length:var(--amp-font-size-xs)] text-[var(--amp-color-dimmed)]">{label}</span>
                <span className="break-all text-[length:var(--amp-font-size-sm)]">{value}</span>
              </div>
            ))}
          </div>
          <div className="flex gap-2 border-t border-[var(--amp-color-default-border)] p-3">
            <Button size="sm" variant="primary" className="flex-1" onPress={onOpen}>
              <ExternalLink size={14} /> Open
            </Button>
            <Button size="sm" variant="secondary" className="flex-1" onPress={onRename}>
              <Pencil size={14} /> Rename
            </Button>
          </div>
        </>
      ) : (
        <div className="px-3 text-[length:var(--amp-font-size-sm)] text-[var(--amp-color-dimmed)]">
          Select an amplifier to see its details.
        </div>
      )}
    </div>
  );
}
