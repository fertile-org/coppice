import React from "react";

// The subset of web/src/components/TicketMarkdown.tsx a plan uses: headings,
// paragraphs, bullet lists and GFM task items (a disabled checkbox inside a
// list-disc item, as remark-gfm renders them there). One string per line.
// `first:mt-0` from the app is applied by index instead.

type Block =
  | { kind: "h1" | "h2" | "h3" | "p"; text: string }
  | { kind: "ul"; items: Array<{ text: string; task: boolean; checked: boolean }> };

const parse = (lines: string[]): Block[] => {
  const blocks: Block[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    const item = /^[-*]\s+(\[( |x|X)\]\s+)?(.*)$/.exec(line);
    if (item) {
      const last = blocks[blocks.length - 1];
      const entry = { text: item[3], task: Boolean(item[1]), checked: /x/i.test(item[2] ?? "") };
      if (last && last.kind === "ul") last.items.push(entry);
      else blocks.push({ kind: "ul", items: [entry] });
      continue;
    }
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({ kind: (["h1", "h2", "h3"] as const)[heading[1].length - 1], text: heading[2] });
      continue;
    }
    blocks.push({ kind: "p", text: line });
  }
  return blocks;
};

export const Markdown: React.FC<{ lines: string[] }> = ({ lines }) => (
  <>
    {parse(lines).map((block, i) => {
      const first = i === 0;
      switch (block.kind) {
        case "h1":
          return (
            <h1 key={i} className={`mb-3 ${first ? "mt-0" : "mt-5"} font-display text-lg font-semibold text-text-primary`}>
              {block.text}
            </h1>
          );
        case "h2":
          return (
            <h2 key={i} className={`mb-2 ${first ? "mt-0" : "mt-5"} font-display text-base font-semibold text-text-primary`}>
              {block.text}
            </h2>
          );
        case "h3":
          return (
            <h3 key={i} className={`mb-2 ${first ? "mt-0" : "mt-4"} font-body text-sm font-semibold text-text-primary`}>
              {block.text}
            </h3>
          );
        case "p":
          return (
            <p key={i} className="mb-3 font-body text-sm leading-relaxed text-text-primary last:mb-0">
              {block.text}
            </p>
          );
        case "ul":
          return (
            <ul key={i} className="mb-3 list-disc space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-primary">
              {block.items.map((it, j) => (
                <li key={j} className="leading-relaxed">
                  {it.task && (
                    <>
                      <input type="checkbox" checked={it.checked} disabled readOnly />{" "}
                    </>
                  )}
                  {it.text}
                </li>
              ))}
            </ul>
          );
      }
    })}
  </>
);
