import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import type { Node, NodeProps } from "@xyflow/react";
import { Slider, Spinner } from "@heroui/react";
import { Layers, Speaker, Volume2, VolumeX } from "lucide-react";
import type { ChannelSpeakerState, ProjectSpeaker, SpeakerGroup } from "../lib/bindings";
import { speakerFields } from "../lib/speakers";
import { CommitNumberInput } from "./CommitNumberInput";
import { Hint } from "./Hint";
import { MUTED } from "./SpeakerBench";
import { FIELD_INPUT } from "./fieldClasses";

/** Width of a tile, amp or speaker: a picture and a name, like ArmoníaPlus's.
 * Everything else about it is in its tooltip, and what can be done to it is in
 * the mode bar. */
export const TILE_W = 96;
/** A parallel group's tile: wide enough for a row of four cabinets and their cables. */
export const GROUP_TILE_W = 136;
const CABINETS_PER_ROW = 4;

/** A speaker tile's picture: one cabinet, or a parallel group's cabinets in
 * rows of four with a little cable between neighbours — cosmetic, the group
 * is linked as one. */
function Cabinets({ count }: { count: number }) {
  if (count <= 1) return <Speaker size={30} className="text-[var(--amp-color-dimmed)]" />;
  return (
    // Exactly four cabinets and three cables wide (4 × 20 + 3 × 10), so the fifth wraps.
    <div className="flex w-[110px] flex-wrap justify-center gap-y-0.5" aria-label={`${count} cabinets in parallel`}>
      {Array.from({ length: count }, (_, i) => (
        <span key={i} className="flex items-center">
          {i % CABINETS_PER_ROW > 0 && (
            <svg width="10" height="8" viewBox="0 0 10 8" aria-hidden className="shrink-0">
              <path d="M0 3 Q5 9 10 3" fill="none" stroke="var(--amp-color-dimmed)" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          )}
          <Speaker size={20} className="text-[var(--amp-color-dimmed)]" />
        </span>
      ))}
    </div>
  );
}

/** Design's two tools: Select moves and picks tiles, Link shows the amps'
 * outputs and the speakers' ways so they can be joined. */
export type Tool = "select" | "link";

/** Which ways of which speaker are waiting for an output to be picked. */
export interface Armed {
  speakerId: string;
  ways: number[];
}

const TILE =
  "relative flex w-full min-w-0 cursor-pointer flex-col items-center gap-1 rounded-xl border border-solid bg-background p-2 outline-none focus-visible:ring-2 focus-visible:ring-accent";
const NAME = "line-clamp-2 w-full text-center text-xs font-medium break-words";
const frame = (lit: boolean) => (lit ? "border-accent bg-accent-soft" : "border-[var(--amp-color-default-border)]");

/** The corner dot: a filled colour, or hollow for "nothing to say"; `blink`
 * for a state that wants attention (an amp out of step with the project). */
function Dot({ color, blink = false }: { color: string | null; blink?: boolean }) {
  return (
    <span
      className={`absolute top-1.5 right-1.5 size-2.5 rounded-full border border-solid ${blink ? "animate-pulse motion-reduce:animate-none" : ""}`}
      style={color ? { background: color, borderColor: color } : { borderColor: "var(--amp-color-dimmed)" }}
    />
  );
}

/** A tile's tooltip: labelled fields, one under the other. */
function TileTooltip({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <div className="flex max-w-[280px] flex-col gap-1.5 py-1 text-left">
      {rows.map(([label, value]) => (
        <div key={label} className="flex flex-col">
          <span className={`${MUTED} uppercase`}>{label}</span>
          <span className="text-sm">{value}</span>
        </div>
      ))}
    </div>
  );
}

function statusText(state: ChannelSpeakerState | undefined): string {
  const status = state?.status;
  if (!status) return "";
  const fields = speakerFields(status);
  const label = status.kind === "match" ? "Match" : status.kind === "edited" ? "Edited" : status.kind === "libraryUpdated" ? "Library updated" : "Detached";
  return ` · ${label}${fields.length ? ` (${fields.join(", ")})` : ""}`;
}

/** What a speaker tile needs to know beyond its own `data`. Handed down by
 * context, so node data stays plain values and a node never holds a stale
 * callback. */
export interface SpeakerCanvas {
  tool: Tool;
  armed: Armed | null;
  /** The speaker whose name is being edited on its tile. */
  renaming: string | null;
  /** The speaker whose output square is under the pointer in the amp list. */
  highlighted: string | null;
  arm: (armed: Armed) => void;
  /** Ends renaming; an empty or unchanged name is left alone. */
  rename: (speaker: ProjectSpeaker, name: string) => void;
  /** Members of the selected group: lit, so the group shows what it runs. */
  groupLit: string[];
  /** Ends renaming a group, as `rename`. */
  renameGroup: (group: SpeakerGroup, name: string) => void;
  setGroupLevels: (group: SpeakerGroup, levels: GroupLevels) => void;
}

/** A change to a group's control; what is left out stays. */
export interface GroupLevels {
  gainDb?: number;
  delayMs?: number;
  muted?: boolean;
}

/** The gain fader's travel on a group tile. The amp's own range is wider; the
 * bar's field reaches all of it. */
export const GROUP_FADER = { min: -40, max: 12, step: 0.5 } as const;
/** The fader's height on a group tile. */
export const GROUP_FADER_H = 120;

export const SpeakerCanvasContext = createContext<SpeakerCanvas | null>(null);

export type SpeakerNodeData = {
  speaker: ProjectSpeaker;
  /** Its library entry's fields; just `model` (the stored label) when the
   * entry is not on this machine. */
  brand: string;
  family: string;
  model: string;
  application: string;
  /** Its library entry is on this machine, so its ways can be applied. */
  linkable: boolean;
  busy: boolean;
  /** Being written to its amp right now: blurred under a spinner. */
  pending: boolean;
  /** The name of the group it is in. */
  group: string | null;
  ways: Array<{
    wayIndex: number;
    label: string;
    /** The output this way is on: its key and "Amp · A". */
    output: { key: string; label: string } | null;
    state?: ChannelSpeakerState;
  }>;
};
export type SpeakerNodeType = Node<SpeakerNodeData, "speaker">;

/** One project speaker. In the Link tool a click arms all its ways and a way
 * chip arms one; the output is then picked on an amp tile. */
function SpeakerTile({ data, selected }: NodeProps<SpeakerNodeType>) {
  const canvas = useContext(SpeakerCanvasContext)!;
  const { speaker, ways } = data;
  const linking = canvas.tool === "link";
  const armed = canvas.armed?.speakerId === speaker.id ? canvas.armed.ways : [];
  const canArm = linking && data.linkable && !data.busy;
  const lit = linking ? armed.length > 0 || canvas.highlighted === speaker.id : !!selected || canvas.groupLit.includes(speaker.id);

  // The dot is for what needs doing (Re-apply), in every tool. Being linked
  // is not a status: it says nothing about the amp, which may be offline.
  const kinds = ways.map((w) => w.state?.status.kind);
  const dot = kinds.includes("edited") ? "var(--amp-color-orange-6)" : kinds.includes("libraryUpdated") ? "var(--accent)" : null;
  // Link state is the Link tool's subject, shown as on the amp card: green = linked.
  const allLinked = linking && ways.length > 0 && ways.every((w) => w.output);

  const field = (label: string, value: string): Array<[string, ReactNode]> => (value ? [[label, value]] : []);
  const tooltip = (
    <TileTooltip
      rows={[
        // The name only when it says more than the model it defaults to.
        ...field("Name", speaker.name === data.model ? "" : speaker.name),
        ...field("Brand", data.brand),
        ...field("Family", data.family),
        ...field("Model", data.model),
        ...field("Application", data.application),
        ...field("Parallel", (speaker.parallel ?? 1) > 1 ? `${speaker.parallel} cabinets on the same outputs` : ""),
        ...field("Group", data.group ?? ""),
        ...ways.map((w): [string, ReactNode] => [
          ways.length > 1 ? `${w.label || `Way ${w.wayIndex + 1}`} output` : "Output",
          w.output ? `${w.output.label}${statusText(w.state)}` : "Not linked",
        ]),
        ...field("Library", data.linkable ? "" : "Not in this machine's library"),
      ]}
    />
  );

  return (
    <Hint text={tooltip} className="flex w-full">
      <div
        className={`${TILE} ${frame(lit)}`}
        style={
          allLinked && !lit
            ? { borderColor: "var(--amp-color-green-filled)", background: "color-mix(in srgb, var(--amp-color-green-filled) 22%, transparent)" }
            : undefined
        }
        onClick={() => canArm && canvas.arm({ speakerId: speaker.id, ways: ways.map((w) => w.wayIndex) })}
      >
        {dot && <Dot color={dot} />}
        <PendingVeil pending={data.pending} />
        <Cabinets count={speaker.parallel ?? 1} />
        {canvas.renaming === speaker.id ? (
          <input
            autoFocus
            aria-label="Speaker name"
            className={`${FIELD_INPUT} nodrag w-full text-center text-xs`}
            defaultValue={speaker.name}
            maxLength={40}
            onClick={(e) => e.stopPropagation()}
            onBlur={(e) => canvas.rename(speaker, e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        ) : (
          <span className={NAME}>{speaker.name}</span>
        )}
        {linking && ways.length > 1 && (
          <div className="flex flex-wrap justify-center gap-px">
            {ways.map((way) => (
              <button
                key={way.wayIndex}
                type="button"
                disabled={!canArm}
                aria-pressed={armed.length === 1 && armed[0] === way.wayIndex}
                aria-label={`Link only ${way.label || "this way"} of ${speaker.name}`}
                className={`${SQUARE} ${armed.includes(way.wayIndex) ? "border-accent bg-accent text-accent-foreground" : way.output ? HELD : FREE}`}
                onClick={(e) => {
                  e.stopPropagation();
                  canvas.arm({ speakerId: speaker.id, ways: [way.wayIndex] });
                }}
              >
                {way.label || "Full"}
              </button>
            ))}
          </div>
        )}
      </div>
    </Hint>
  );
}

/** Blurs a tile under a spinner while it is written. Always mounted, so it
 * fades in and out instead of popping. */
function PendingVeil({ pending }: { pending: boolean }) {
  return (
    <div
      aria-hidden={!pending}
      className={`absolute inset-0 z-10 flex items-center justify-center rounded-xl bg-background/40 backdrop-blur-sm transition-opacity duration-300 ${
        pending ? "opacity-100" : "pointer-events-none opacity-0"
      }`}
    >
      <Spinner size="sm" />
    </div>
  );
}

/** Width of a group's control tile. */
export const GROUP_CONTROL_W = 184;

export type GroupNodeData = {
  group: SpeakerGroup;
  /** Being written to its members' amps: the controls wait. */
  busy: boolean;
};
export type GroupNodeType = Node<GroupNodeData, "group">;

const formatDb = (db: number) => `${db > 0 ? "+" : ""}${db.toFixed(1)} dB`;

/** A group's control, a tile like a speaker's: its picture and name, then a
 * gain fader, its gain and delay, and one mute for every member. Selecting it
 * lights its members and puts gain and delay fields in the speaker bar. */
function GroupTile({ data, selected }: NodeProps<GroupNodeType>) {
  const canvas = useContext(SpeakerCanvasContext)!;
  const { group, busy } = data;
  const gain = group.gainDb ?? 0;
  const delay = group.delayMs ?? 0;
  const muted = !!group.muted;
  // The fader moves locally and writes once, when it is let go.
  const [draft, setDraft] = useState(gain);
  useEffect(() => setDraft(gain), [gain]);
  const disabled = busy || canvas.tool !== "select";
  const count = group.speakerIds.length;

  return (
    <Hint
      text={
        <TileTooltip
          rows={[
            ["Group", group.name],
            ["Speakers", String(count)],
            ["Gain", `${formatDb(gain)} on every member output's volume`],
            ["Delay", `${delay.toFixed(2)} ms on top of every member's own`],
            ["Mute", muted ? "Every member output is muted" : "Off"],
          ]}
        />
      }
      className="flex w-full"
    >
      <div className={`${TILE} ${frame(!!selected)} cursor-default`}>
        {muted && <Dot color="var(--amp-color-red-filled)" />}
        <PendingVeil pending={busy} />
        <Layers size={30} className="text-[var(--amp-color-dimmed)]" />
        {canvas.renaming === group.id ? (
          <input
            autoFocus
            aria-label="Group name"
            className={`${FIELD_INPUT} nodrag w-full text-center text-xs`}
            defaultValue={group.name}
            maxLength={40}
            onClick={(e) => e.stopPropagation()}
            onBlur={(e) => canvas.renameGroup(group, e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        ) : (
          <span className={NAME}>{group.name}</span>
        )}
        <span className={`${MUTED} -mt-1`}>{count === 1 ? "1 speaker" : `${count} speakers`}</span>
        <div className="nodrag nowheel flex w-full items-stretch gap-2 pt-1" onClick={(e) => e.stopPropagation()}>
          {/* The fader, as the limiter's: moves locally, writes once when let go. */}
          <div
            className="flex flex-col items-center gap-1"
            style={{ opacity: disabled ? 0.45 : 1 }}
            onDoubleClick={() => !disabled && canvas.setGroupLevels(group, { gainDb: 0 })}
          >
            <Slider
              aria-label={`${group.name} gain`}
              orientation="vertical"
              style={{ height: GROUP_FADER_H }}
              minValue={GROUP_FADER.min}
              maxValue={GROUP_FADER.max}
              step={GROUP_FADER.step}
              value={Math.min(GROUP_FADER.max, Math.max(GROUP_FADER.min, draft))}
              isDisabled={disabled}
              onChange={(v) => setDraft(v as number)}
              onChangeEnd={(v) => v !== gain && canvas.setGroupLevels(group, { gainDb: v as number })}
            >
              <Slider.Track>
                <Slider.Fill />
                <Slider.Thumb />
              </Slider.Track>
            </Slider>
            <span className="text-[10px] tabular-nums text-[var(--amp-color-dimmed)]">{formatDb(draft)}</span>
          </div>
          <div className="flex min-w-0 flex-1 flex-col justify-center gap-2">
            <label className="flex flex-col gap-0.5">
              <span className={`${MUTED} uppercase`}>Gain</span>
              <CommitNumberInput
                className="py-0.5 tabular-nums"
                value={Number(gain.toFixed(2))}
                min={-80}
                max={18}
                step={0.5}
                suffix=" dB"
                onCommit={(gainDb) => !disabled && canvas.setGroupLevels(group, { gainDb })}
              />
            </label>
            <label className="flex flex-col gap-0.5">
              <span className={`${MUTED} uppercase`}>Delay</span>
              <CommitNumberInput
                className="py-0.5 tabular-nums"
                value={Number(delay.toFixed(2))}
                min={0}
                max={20}
                step={0.01}
                suffix=" ms"
                onCommit={(delayMs) => !disabled && canvas.setGroupLevels(group, { delayMs })}
              />
            </label>
          </div>
        </div>
        <button
          type="button"
          aria-pressed={muted}
          aria-label={muted ? `Unmute ${group.name}` : `Mute ${group.name}`}
          disabled={disabled}
          className={`nodrag flex h-6 w-full cursor-pointer items-center justify-center gap-1 rounded-md border border-solid text-[11px] font-semibold disabled:cursor-default disabled:opacity-50 ${
            muted ? "border-[var(--amp-color-red-filled)] bg-[var(--amp-color-red-filled)] text-white" : `${FREE} bg-transparent`
          }`}
          onClick={(e) => {
            e.stopPropagation();
            canvas.setGroupLevels(group, { muted: !muted });
          }}
        >
          {muted ? <VolumeX size={12} /> : <Volume2 size={12} />}
          {muted ? "Muted" : "Mute"}
        </button>
      </div>
    </Hint>
  );
}

// Module level: React Flow re-mounts every node when this changes identity.
export const NODE_TYPES = { speaker: SpeakerTile, group: GroupTile };

const SQUARE = "h-[18px] min-w-[18px] cursor-pointer rounded-sm border border-solid px-1 text-[10px] leading-none font-bold disabled:cursor-default";
// A linked way: the same fixed green as a linked output on the amp card.
const HELD = "border-[var(--amp-color-green-filled)] bg-[var(--amp-color-green-filled)] text-white";
const FREE = "border-[var(--amp-color-default-border)]";

/** One output of an amp card. In the Link tool a `target` can be picked, a
 * `blocked` one can't start the armed speaker's span, and `idle` means
 * nothing is armed — a click then selects it for bridging. */
export interface OutputBox {
  key: string;
  label: string;
  /** What it holds; `null` when free. */
  text: string | null;
  mode: "idle" | "target" | "blocked";
  /** Lit: its speaker is hovered, or a pick would fill it. */
  lit: boolean;
  /** Linked to a project speaker — green. An output holding a speaker set up
   * on the amp only (`text` without a link) is not. */
  linked: boolean;
  /** A bridged pair: one output, twice as wide. */
  bridged: boolean;
  /** Being written right now: spins. */
  pending: boolean;
  /** Selected for Bridge / Unbridge. */
  picked: boolean;
}

// The end sections take the card's corners, so an inset ring follows its radius instead of being clipped.
const SEGMENT_EDGE = "border-0 border-l border-solid border-[var(--amp-color-default-border)] first:border-l-0 first:rounded-l-md last:rounded-r-md";

/** One project amp in the Workspace's amp list: a rack card, its name in the
 * middle and its outputs across the whole width under it — a thin strip of
 * held/free in the Select tool, labelled sections to pick in the Link tool,
 * as in ArmoníaPlus. */
export function AmpTile({
  name,
  isCvr,
  dot,
  blink,
  tooltip,
  selected,
  linking,
  boxes,
  onSelect,
  onOpen,
  onPick,
  onBoxHover,
}: {
  name: string;
  isCvr: boolean;
  /** Its sync state as a colour; `null` when there is nothing to compare. */
  dot: string | null;
  /** The dot blinks: the amp differs from the project. */
  blink?: boolean;
  tooltip: Array<[string, ReactNode]>;
  selected: boolean;
  linking: boolean;
  boxes: OutputBox[];
  onSelect: () => void;
  onOpen: () => void;
  /** `additive`: Ctrl or Shift was held. */
  onPick: (key: string, additive: boolean) => void;
  onBoxHover: (key: string | null) => void;
}) {
  return (
    <Hint text={<TileTooltip rows={tooltip} />} className="flex min-w-0">
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        className={`relative flex h-10 w-full min-w-0 cursor-pointer overflow-hidden rounded-md border border-solid bg-background outline-none focus-visible:ring-2 focus-visible:ring-accent ${frame(
          selected || boxes.some((b) => b.lit),
        )}`}
        onClick={onSelect}
        onDoubleClick={onOpen}
        onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && onOpen()}
      >
        {/* The front panel as the card's background. */}
        {isCvr && <img src="/cvr_dsp_amp.png" alt="" className="pointer-events-none absolute inset-0 size-full object-cover opacity-35" />}
        {linking ? (
          // The outputs take the name's place, one section each across the whole card.
          <div className="relative flex min-w-0 flex-1">
            {boxes.map((box) => (
              <button
                key={box.key}
                type="button"
                disabled={box.mode === "blocked" || box.pending}
                aria-pressed={box.picked}
                aria-label={`Output ${box.label} of ${name}${box.text ? `, holds ${box.text}` : ", free"}`}
                style={{
                  flex: box.bridged ? 2 : 1,
                  // Linked: the fixed status green, translucent over the panel.
                  background: box.lit || box.picked || !box.linked ? undefined : "color-mix(in srgb, var(--amp-color-green-filled) 70%, transparent)",
                }}
                className={`relative flex min-w-0 cursor-pointer items-center justify-center text-xs font-bold disabled:cursor-default ${SEGMENT_EDGE} ${
                  box.lit || box.picked ? "bg-accent text-accent-foreground" : ""
                } ${box.linked && !box.lit && !box.picked ? "text-white" : ""} ${box.mode === "target" ? "ring-2 ring-accent ring-inset" : ""} ${
                  box.mode === "blocked" ? "opacity-40" : ""
                }`}
                onClick={(e) => {
                  e.stopPropagation();
                  onPick(box.key, e.ctrlKey || e.metaKey || e.shiftKey);
                }}
                // A double-click on an output is two picks, not "open the amp".
                onDoubleClick={(e) => e.stopPropagation()}
                onMouseEnter={() => onBoxHover(box.key)}
                onMouseLeave={() => onBoxHover(null)}
                onFocus={() => onBoxHover(box.key)}
                onBlur={() => onBoxHover(null)}
              >
                {/* Letter and spinner cross-fade; both stay mounted. */}
                <span className={`transition-opacity duration-300 ${box.pending ? "opacity-0" : "opacity-100"}`}>{box.label}</span>
                <span
                  aria-hidden={!box.pending}
                  className={`absolute inset-0 flex items-center justify-center transition-opacity duration-300 ${box.pending ? "opacity-100" : "opacity-0"}`}
                >
                  <Spinner size="sm" />
                </span>
              </button>
            ))}
          </div>
        ) : (
          <>
            <span className="relative min-w-0 flex-1 self-center truncate px-6 text-center text-sm font-semibold">{name}</span>
            <Dot color={dot} blink={blink} />
          </>
        )}
      </div>
    </Hint>
  );
}
