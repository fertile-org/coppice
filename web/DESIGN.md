# Coppice Design Direction

> *Grow an agent team from shared roots.*

Coppice is a self-hosted agent workspace with a Trello-like board. The visual language draws from **coppice forestry** — managed woodland where new growth springs from shared stumps. The UI should feel **organic, warm, and alive**, not like generic purple SaaS.

## Aesthetic direction

**Tone:** Organic / editorial forest journal. Grounded earth tones with living green accents. Paper-like surfaces with subtle grain. Restrained motion — growth, not flash.

**Differentiation:** Warm bark browns and cream paper backgrounds immediately signal "forest workshop" rather than "startup dashboard." Moss green is the single accent — used sparingly for actions, active states, and Done/Ready columns.

## Typography

| Role | Family | Rationale |
|------|--------|-----------|
| Display | **Fraunces** | Soft, slightly wonky variable serif with organic character. Headings feel hand-set, like field notes. |
| Body | **Newsreader** | Editorial serif optimized for long reading. Comments, descriptions, and ticket bodies stay comfortable at length. |

**Avoid:** Inter, Roboto, Arial, Space Grotesk, system-ui stacks as primary faces.

**Pairing rules:**
- Headings: Fraunces 600–700, tight leading
- Body: Newsreader 400–500, relaxed leading (1.625)
- Mono: system monospace for branch names, IDs, code snippets only

## Color palette

### Core

| Token | Hex | Use |
|-------|-----|-----|
| `--color-bark-900` | `#2a1f18` | Primary text |
| `--color-bark-600` | `#6b5344` | Secondary text |
| `--color-moss-600` | `#4a7c59` | Primary accent (buttons, links, focus) |
| `--color-moss-700` | `#3d6b4f` | Accent hover |
| `--color-paper-50` | `#faf7f2` | Page background |
| `--color-paper-100` | `#f5f0e6` | Card/surface background |
| `--color-surface-raised` | `#fffdf9` | Elevated panels |

### Rationale

- **Bark scale** — warm browns from stump to weathered bark. Never cold gray.
- **Moss scale** — living green for growth, success, agent activity. Not neon.
- **Paper scale** — cream/off-white surfaces evoke notebook paper in a woodland shed.

### Semantic

- **Danger:** muted terracotta-red (`#9b3d3d`) — blocked tickets, errors
- **Warning:** harvest gold (`#b8860b`) — QA, medium priority
- **Info:** slate-blue (`#3d6b8b`) — in-progress, system comments

## Board column colors

Each column has a distinct but harmonious tint — muted, not saturated traffic lights.

| Column | Background | Accent | Character |
|--------|------------|--------|-----------|
| Backlog | Warm stone `#e8e4df` | `#6b6560` | Untouched wood, waiting |
| Ready | Light moss `#e0ebe4` | `#4a7c59` | Sprouts ready to plant |
| In Progress | Sky-slate `#dce8f0` | `#3d6b8b` | Work under open canopy |
| In Review | Soft violet `#ede5f5` | `#6b4f8b` | Second eyes, twilight |
| In QA | Harvest wheat `#f5edd6` | `#9a7b2e` | Testing the harvest |
| Wait for Final Review | Aged parchment `#f0e8dc` | `#8b6914` | Almost ripe |
| Done | Full moss `#d4ead9` | `#2d5a3d` | Mature growth |
| Blocked | Wilted rose `#f5e6e6` | `#9b3d3d` | Dead branch — needs attention |

Column headers use accent color for the status dot; cards sit on `--color-surface-raised` with `--shadow-card`.

## Badge variants

### Priority

| Level | Feel |
|-------|------|
| Low | Moss tint — gentle, can wait |
| Medium | Wheat/gold — ripening |
| High | Terracotta — needs sun soon |
| Critical | Rose-red — wilt risk |

### Comment author type

| Type | Feel |
|------|------|
| Human | Bark on paper — grounded collaborator |
| Agent | Moss — AI growth from the coppice |
| System | Slate-blue — automated forest keeper |

### Agent status

| State | Colors |
|-------|--------|
| Active | Moss background, dark green text |
| Disabled | Bark-muted, faded |

## Texture & atmosphere

- **Grain overlay:** `.coppice-grain` applies a fixed SVG noise layer at 3.5% opacity — subtle paper tooth without performance cost.
- **Shadows:** Warm brown-tinted shadows (`rgba(42, 31, 24, …)`) — never pure black.
- **Borders:** `--color-bark-200` default; stronger on focus with moss ring.

## Spacing & layout

- Base unit: 4px (`--space-1`)
- Board columns: min 280px, gap `--space-4`
- Card padding: `--space-3` vertical, `--space-4` horizontal
- Page shell: generous `--space-8` margins on desktop

## Motion principles

- Fast transitions (`120ms`) for hovers and focus
- Column drag: subtle lift (`--shadow-lg`) + slight scale
- Page enter: staggered fade-up on board columns (future)
- Prefer CSS transitions; reserve JS animation for dnd-kit

## Implementation

All tokens live in `src/styles/tokens.css`. Tailwind extends theme colors to reference these CSS variables — never hardcode hex in components.

```css
/* Example usage */
background: var(--color-surface);
color: var(--color-text-primary);
border: 1px solid var(--color-border);
```

## What we are NOT

- Purple gradients on white
- Glassmorphism overload
- Dark mode first (light paper is the default forest-shed experience)
- Generic icon-only chrome with no warmth
