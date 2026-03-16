# Bindery — Design language

## Name

**Bindery** — a place where books are bound. Files go in, a book comes out.

Domain: `bindery.audio`

## Personality

- Calm, confident, understated
- Feels like a well-made tool, not a tech product
- Premium without being pretentious
- The kind of app you'd expect a designer to have built

## Colour palette

### Light mode

| Role | Colour | Hex | Usage |
|------|--------|-----|-------|
| Background | Warm white | `#FAFAF8` | App background |
| Surface | Cream | `#F2F0EC` | Cards, panels, drop zone |
| Border | Warm grey | `#E2DFD9` | Dividers, input borders |
| Text primary | Charcoal | `#1A1918` | Headings, body text |
| Text secondary | Warm grey | `#7A756E` | Labels, metadata, hints |
| Accent | Amber | `#C67B30` | Primary actions, progress bar, active states |
| Accent hover | Deep amber | `#A86520` | Button hover, active accent |
| Success | Sage | `#4A7C59` | Completion states |
| Error | Terracotta | `#C45D4E` | Error messages |

### Dark mode

| Role | Colour | Hex | Usage |
|------|--------|-----|-------|
| Background | Deep charcoal | `#1A1918` | App background |
| Surface | Dark warm | `#242320` | Cards, panels, drop zone |
| Border | Warm dark grey | `#3A3835` | Dividers, input borders |
| Text primary | Warm white | `#EDECEA` | Headings, body text |
| Text secondary | Muted warm | `#8A857E` | Labels, metadata, hints |
| Accent | Amber | `#D4893A` | Primary actions, progress bar |
| Accent hover | Light amber | `#E09A4A` | Button hover |
| Success | Sage | `#5A9A6A` | Completion states |
| Error | Soft terracotta | `#D46E5E` | Error messages |

## Typography

- **Brand / headings:** Instrument Serif (Google Fonts) — literary, warm, editorial
  - Fallback: Georgia, serif
- **UI / body:** Inter (system-available on most platforms) — clean, readable, neutral
  - Fallback: system-ui, -apple-system, sans-serif
- **Monospace (file paths, technical):** JetBrains Mono or SF Mono
  - Fallback: ui-monospace, monospace

### Scale

| Element | Font | Weight | Size |
|---------|------|--------|------|
| App title | Instrument Serif | 400 | 20px |
| Section headings | Inter | 600 | 14px |
| Body text | Inter | 400 | 13px |
| Labels / hints | Inter | 400 | 12px |
| File paths | Mono | 400 | 12px |
| Button text | Inter | 500 | 13px |

## Spacing

Base unit: 4px. Use multiples: 4, 8, 12, 16, 24, 32, 48.

- Component padding: 12–16px
- Section gaps: 24px
- Window padding: 24px
- Input height: 36px
- Button height: 36px
- Border radius: 8px (components), 12px (panels/cards)

## Icon style

- App icon concept: a stylised book spine, viewed from above — pages fanning out, suggesting binding/merging
- In-app icons: Lucide icon set — consistent 1.5px stroke weight, matches the minimal aesthetic
- Colour: follow text-secondary by default, accent for interactive elements

## Motion

- Transitions: 150ms ease-out (fast, not jarring)
- Progress bar: smooth linear animation
- Drag-and-drop: subtle scale + shadow on lift (1.02 scale, 4px shadow)
- File list reorder: 200ms spring animation
- No gratuitous animations — everything serves feedback

## Component patterns

### Drop zone

Large, centred, dashed border. Text: "Drop audio files here" with a subtle book/binding icon. On hover/dragover: border goes solid, accent colour, slight background tint.

### File list

Compact rows. Each row: drag handle (left) → chapter number → chapter name (editable) → duration → remove button (right). Zebra striping via alternating surface/background.

### Metadata panel

Collapsible section below the file list. Cover art thumbnail (left), form fields (right). Fields stack vertically: Title, Artist, Album, Narrator, Year.

### Convert button

Full-width at the bottom. Accent background, white text. During conversion: becomes progress bar with percentage + current file name. On completion: turns success colour, shows "Reveal in Finder".

## Principles

1. Nothing competes for attention — one clear action at a time
2. The default state should be correct 90% of the time
3. Dense information, spacious layout
4. Dark mode is not an afterthought — both modes feel intentional
5. Respect the platform — native title bar, native file dialogs, system font fallbacks
