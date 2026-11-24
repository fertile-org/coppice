import React from "react";
import {
  AbsoluteFill,
  Sequence,
  interpolate,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import "../kit/tokens";
import { APP_LOGICAL } from "../kit/app/AppShell";
import {
  AppShell,
  Board,
  CodeReview,
  DARK_MODE_DESIGNS,
  DesignDialog,
  FRONTEND_ENGINEER,
  PLAN,
  PlanPanel,
  TicketCard,
  THEME_TOGGLE_REVIEW,
  type BoardColumn,
  type Ticket,
} from "../kit/app";
import { Caption, Cursor, Focus, TypeText } from "../kit/motion";

/**
 * Marketing hero (~8 s). Same framing throughout (Focus scale 1): whole board,
 * no zoom. Captions match e2e/screenshot/capture-hero.mjs CAPTIONS.
 */

export const HERO_DURATION_FRAMES = 240; // 8 s @ 30 fps

const TOGGLE = "Add a dark mode toggle to Settings";
const DESCRIPTION = [
  "Settings should offer a dark mode toggle and follow the same theme preference as the rest of the app.",
];
const PLAN_LINES = [
  "## Plan",
  "### Approach",
  "Build design option A: a switch in the Settings row. Keep the choice on this machine and apply it through the theme hook the rest of the app already uses.",
  "### Steps",
  "- [ ] Add the Settings toggle",
  "- [ ] Use the existing theme hook",
  "- [ ] Add a settings test",
  "### Risks",
  "None.",
  "### Out of scope",
  "A new color palette.",
];
const APPROVED_LINES = [
  "## Approved plan",
  "### Approach",
  "Build design option A: a switch in the Settings row. Keep the choice on this machine and apply it through the theme hook the rest of the app already uses.",
  "### Steps",
  "- [ ] Add the Settings toggle",
  "- [ ] Use the existing theme hook",
  "- [ ] Add a settings test",
];

const S = APP_LOGICAL.scale;
const cx = (logicalX: number) => logicalX * S;
const cy = (logicalY: number) => logicalY * S;

const SUPPORT: Ticket[] = [
  {
    id: "s1",
    title: "Remember the last open settings tab",
    agent: "PM",
    priority: "low",
    status: PLAN.planNeedsRun,
  },
  {
    id: "s2",
    title: "Tighten spacing on the settings form",
    agent: "Frontend",
    priority: "high",
    live: true,
  },
  {
    id: "s3",
    title: "Review the settings page at a narrow width",
    agent: "QA",
    priority: "medium",
  },
  {
    id: "s4",
    title: "Cover theme switching with a smoke test",
    agent: "QA",
    priority: "medium",
  },
];

const story = (overrides: Partial<Ticket> = {}): Ticket => ({
  id: "story",
  title: TOGGLE,
  agent: "PM",
  priority: "high",
  ...overrides,
});

const emptyBoard = (): BoardColumn[] => [
  { name: "Backlog", tickets: [SUPPORT[0]] },
  { name: "Plan Review", tickets: [] },
  { name: "Ready", tickets: [] },
  { name: "In Progress", tickets: [SUPPORT[1]] },
  { name: "In Review", tickets: [SUPPORT[2]] },
  { name: "In QA", tickets: [SUPPORT[3]] },
  { name: "Wait for Human Review", tickets: [] },
  { name: "Done", tickets: [] },
];

const withStory = (
  column: BoardColumn["name"],
  ticket: Ticket,
  extras?: Partial<Record<BoardColumn["name"], Ticket[]>>,
): BoardColumn[] =>
  emptyBoard().map((col) => {
    if (extras?.[col.name]) return { ...col, tickets: extras[col.name]! };
    if (col.name === column) {
      return {
        ...col,
        tickets: col.name === "Backlog" ? [ticket, ...col.tickets] : [ticket, ...col.tickets],
      };
    }
    return col;
  });

/** Fade between consecutive Sequences that share framing (scale 1). */
const Crossfade: React.FC<{
  children: React.ReactNode;
  durationInFrames: number;
  fade?: number;
}> = ({ children, durationInFrames, fade = 6 }) => {
  const frame = useCurrentFrame();
  const opacity = interpolate(
    frame,
    [0, fade, durationInFrames - fade, durationInFrames],
    [0, 1, 1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
  );
  return <AbsoluteFill style={{ opacity }}>{children}</AbsoluteFill>;
};

// Beat 0–34: write a ticket into Backlog.
const WriteBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const typed = frame >= 8;
  const landed = frame >= 22;
  const columns = landed
    ? withStory("Backlog", story({ status: PLAN.planNeedsRun }))
    : emptyBoard();
  return (
    <Crossfade durationInFrames={34}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <Board columns={columns} highlightedIds={landed ? ["story"] : []} />
        </AppShell>
        {typed && !landed && (
          <div
            className="bg-overlay-strong text-text-primary font-body absolute rounded-md px-3 py-2 text-sm"
            style={{ left: cx(280), top: cy(220) }}
          >
            <TypeText text={TOGGLE} startFrame={8} charsPerSecond={48} />
          </div>
        )}
        <Cursor
          path={[
            { frame: 0, x: cx(400), y: cy(500) },
            { frame: 6, x: cx(320), y: cy(210), click: true },
            { frame: 28, x: cx(360), y: cy(280) },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

// Beat 34–62: PM plan in the drawer.
const PlanBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const lines = Math.min(
    PLAN_LINES.length,
    Math.max(4, Math.floor(interpolate(frame, [0, 24], [4, PLAN_LINES.length]))),
  );
  return (
    <Crossfade durationInFrames={28}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <Board columns={withStory("Plan Review", story({ status: PLAN.waitingForApproval }))} />
          <PlanPanel
            title={TOGGLE}
            description={DESCRIPTION}
            assignee={{ name: "PM", role: "PM" }}
            planMarkdownLines={PLAN_LINES}
            revealedLines={lines}
            approved={false}
            design={{ count: 3, using: { ...DARK_MODE_DESIGNS.options[0], yourChoice: false } }}
          />
        </AppShell>
        <Cursor
          path={[
            { frame: 0, x: cx(360), y: cy(280) },
            { frame: 12, x: cx(900), y: cy(420) },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

// Beat 62–92: brief design-options (keeps videoAlt honest).
const DesignBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const active = frame >= 18 ? "B" : "A";
  return (
    <Crossfade durationInFrames={30}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <Board columns={withStory("Plan Review", story({ status: PLAN.waitingForApproval }))} />
          <PlanPanel
            title={TOGGLE}
            description={DESCRIPTION}
            assignee={{ name: "PM", role: "PM" }}
            planMarkdownLines={PLAN_LINES}
            revealedLines={PLAN_LINES.length}
            approved={false}
            design={{ count: 3, using: { ...DARK_MODE_DESIGNS.options[0], yourChoice: false } }}
          />
          <DesignDialog active={active as "A" | "B" | "C"} />
        </AppShell>
        <Cursor
          path={[
            { frame: 0, x: cx(900), y: cy(420) },
            { frame: 10, x: cx(480), y: cy(160) },
            { frame: 16, x: cx(560), y: cy(160), click: true },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

// Beat 92–118: approve → Ready.
const ApproveBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const approved = frame >= 14;
  return (
    <Crossfade durationInFrames={26}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <Board
            columns={
              approved
                ? withStory("Ready", story({ agent: "Frontend" }))
                : withStory("Plan Review", story({ status: PLAN.waitingForApproval }))
            }
            highlightedIds={approved ? ["story"] : []}
          />
          {!approved && (
            <PlanPanel
              title={TOGGLE}
              description={DESCRIPTION}
              assignee={{ name: "PM", role: "PM" }}
              planMarkdownLines={PLAN_LINES}
              revealedLines={PLAN_LINES.length}
              approved={false}
              design={{ count: 3, using: { ...DARK_MODE_DESIGNS.options[0], yourChoice: false } }}
              approvePressed={frame >= 10 && frame < 16}
            />
          )}
        </AppShell>
        <Cursor
          path={[
            { frame: 0, x: cx(560), y: cy(160) },
            { frame: 8, x: cx(980), y: cy(700) },
            { frame: 12, x: cx(980), y: cy(700), click: true },
            { frame: 22, x: cx(700), y: cy(300) },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

/**
 * Beat 118–168: card springs Ready → In Progress → In Review → In QA → Wait.
 * Board is scrolled so those columns sit in frame; the flying card uses springs.
 */
const BuildBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  // Column card anchors inside the scrolled board (logical → canvas).
  const stops = [
    { at: 0, x: cx(300), y: cy(260), column: "Ready" as const, agent: "Frontend", live: false },
    { at: 12, x: cx(520), y: cy(260), column: "In Progress" as const, agent: "Frontend", live: true },
    { at: 24, x: cx(740), y: cy(260), column: "In Review" as const, agent: "Frontend", live: false },
    { at: 36, x: cx(960), y: cy(260), column: "In QA" as const, agent: "QA", live: false },
    {
      at: 48,
      x: cx(300),
      y: cy(260),
      column: "Wait for Human Review" as const,
      agent: "Frontend",
      live: false,
    },
  ];

  let from = stops[0];
  let to = stops[0];
  for (let i = 1; i < stops.length; i++) {
    if (frame >= stops[i].at) {
      from = stops[i];
      to = stops[i];
    } else {
      to = stops[i];
      break;
    }
  }
  const local = Math.max(0, frame - from.at);
  const progress = spring({
    frame: local,
    fps,
    config: { damping: 18, stiffness: 120, mass: 0.8 },
  });
  const x = from.x + (to.x - from.x) * (from === to ? 1 : progress);
  const y = from.y + (to.y - from.y) * (from === to ? 1 : progress) + (from === to ? 0 : (1 - progress) * -8);

  const parked = from === to || progress > 0.92;
  const current = parked ? to : from;

  // Scroll so Ready is near the left of the content; after Wait stop, scroll further.
  const scrollLogical = current.column === "Wait for Human Review" ? 1180 : 560;
  const columns = emptyBoard().map((col) => {
    if (col.name === current.column && parked) {
      return {
        ...col,
        tickets: [
          story({ agent: current.agent, live: current.live }),
          ...col.tickets.filter((t) => t.id !== "story"),
        ],
      };
    }
    if (col.name === "Ready" || col.name === "In Progress") {
      return { ...col, tickets: col.tickets.filter((t) => t.id !== "s2" || col.name !== "In Progress") };
    }
    return col;
  });

  return (
    <Crossfade durationInFrames={50} fade={5}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <div style={{ translate: `${-scrollLogical * S}px 0px` }}>
            <Board columns={columns} />
          </div>
        </AppShell>
        {!parked && (
          <div
            className="pointer-events-none absolute w-72"
            style={{
              left: x,
              top: y,
              scale: interpolate(progress, [0, 0.5, 1], [1, 1.04, 1]),
            }}
          >
            <TicketCard
              ticket={story({ agent: to.agent, live: to.live })}
              highlighted
            />
          </div>
        )}
        {parked && (
          <div
            className="pointer-events-none absolute"
            style={{
              left: to.x - 4,
              top: to.y - 4,
              width: 288 * S + 8,
              height: 110,
              outline: "3px solid var(--color-warning)",
              outlineOffset: 0,
              borderRadius: 8,
              opacity: spring({
                frame: local,
                fps,
                config: { damping: 20, stiffness: 160 },
              }),
            }}
          />
        )}
        <Cursor
          path={[
            { frame: 0, x: cx(700), y: cy(300) },
            { frame: 20, x: cx(600), y: cy(320) },
            { frame: 45, x: cx(420), y: cy(300) },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

// Beat 168–208: comment on a diff line.
const DiffBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const clickAt = 10;
  const typeAt = 14;
  const addAt = 32;
  const composing = frame > clickAt && frame < addAt + 2;
  const posted = frame >= addAt + 2;
  const cursorY = cy(420);
  return (
    <Crossfade durationInFrames={40}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <CodeReview
          comment={
            composing
              ? {
                  line: 8,
                  mode: "compose",
                  body: <TypeText text={THEME_TOGGLE_REVIEW.comment} startFrame={typeAt} />,
                  hasText: frame > typeAt + 2,
                  addPressed: frame >= addAt - 1,
                }
              : posted
                ? {
                    line: 8,
                    mode: "posted",
                    body: THEME_TOGGLE_REVIEW.comment,
                    hasText: true,
                  }
                : undefined
          }
        />
        <Cursor
          path={[
            { frame: 0, x: cx(900), y: cy(500) },
            {
              frame: clickAt,
              x: cx(640),
              y: cursorY,
              click: true,
            },
            { frame: addAt - 4, x: cx(600), y: cy(520) },
            { frame: addAt, x: cx(600), y: cy(520), click: true },
          ]}
        />
        {/* Soft spring settle after the comment posts */}
        {posted && (
          <div
            style={{
              opacity: spring({
                frame: frame - addAt,
                fps,
                config: { damping: 200 },
              }),
            }}
          />
        )}
      </Focus>
    </Crossfade>
  );
};

// Beat 208–240: Accept → Done hold (poster frame).
const AcceptBeat: React.FC = () => {
  const frame = useCurrentFrame();
  const accepted = frame >= 12;
  const columns = accepted
    ? withStory("Done", story({ agent: "Frontend" }), {
        "Wait for Human Review": [],
        "In Progress": [SUPPORT[1]],
      })
    : withStory("Wait for Human Review", story({ agent: "Frontend" }), {
        "In Progress": [SUPPORT[1]],
      });
  return (
    <Crossfade durationInFrames={32} fade={4}>
      <Focus keyframes={[{ frame: 0, x: cx(720), y: cy(400), scale: 1 }]}>
        <AppShell>
          <div style={{ translate: accepted ? `${-1400 * S}px 0px` : `${-1180 * S}px 0px` }}>
            <Board columns={columns} highlightedIds={["story"]} />
          </div>
          {!accepted && (
            <PlanPanel
              title={TOGGLE}
              description={[...DESCRIPTION, "", ...APPROVED_LINES]}
              assignee={FRONTEND_ENGINEER}
              planMarkdownLines={APPROVED_LINES}
              revealedLines={APPROVED_LINES.length}
              approved
              design={{ count: 3, using: { ...DARK_MODE_DESIGNS.options[0], yourChoice: false } }}
              status="Wait for Human Review"
            />
          )}
        </AppShell>
        <Cursor
          path={[
            { frame: 0, x: cx(600), y: cy(520) },
            { frame: 8, x: cx(980), y: cy(120) },
            { frame: 11, x: cx(980), y: cy(120), click: true },
            { frame: 24, x: cx(500), y: cy(300) },
          ]}
        />
      </Focus>
    </Crossfade>
  );
};

const CAPTIONS = [
  { text: "Write a ticket.", from: 0, durationInFrames: 34 },
  { text: "Your PM agent drafts a plan.", from: 34, durationInFrames: 28 },
  { text: "It shows you design options.", from: 62, durationInFrames: 30 },
  { text: "You approve it.", from: 92, durationInFrames: 26 },
  { text: "Agents build, review, and test.", from: 118, durationInFrames: 50 },
  { text: "Review the diff. Comment on any line.", from: 168, durationInFrames: 40 },
  { text: "Nothing is done until you accept it.", from: 208, durationInFrames: 32 },
] as const;

export const Hero: React.FC = () => (
  <AbsoluteFill className="bg-background">
    <Sequence durationInFrames={34}>
      <WriteBeat />
    </Sequence>
    <Sequence from={34} durationInFrames={28} premountFor={12}>
      <PlanBeat />
    </Sequence>
    <Sequence from={62} durationInFrames={30} premountFor={12}>
      <DesignBeat />
    </Sequence>
    <Sequence from={92} durationInFrames={26} premountFor={12}>
      <ApproveBeat />
    </Sequence>
    <Sequence from={118} durationInFrames={50} premountFor={12}>
      <BuildBeat />
    </Sequence>
    <Sequence from={168} durationInFrames={40} premountFor={12}>
      <DiffBeat />
    </Sequence>
    <Sequence from={208} durationInFrames={32} premountFor={12}>
      <AcceptBeat />
    </Sequence>
    {CAPTIONS.map((c) => (
      <Caption key={c.text} text={c.text} from={c.from} durationInFrames={c.durationInFrames} />
    ))}
  </AbsoluteFill>
);
