---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# TUI Component Model

## Context

The acai-ts CLI tool needs a terminal UI that supports rich rendering, interactive components, and differential updates for smooth performance. The UI must handle scrolling, modals, user input, and mouse events.

## Decision

### Component Interface

All UI components implement a common interface:

```typescript
export interface Component {
  render(width: number): string[];
  handleInput?(data: string): void;
  getCursorPosition?(): [number, number] | null;
  wantsNavigationKeys?(): boolean;
}
```

### Rendering Model

Components render to arrays of strings (lines) for a given viewport width:

```typescript
class AssistantMessage implements Component {
  render(width: number): string[] {
    return wrapText(this.content, width).lines;
  }
}
```

### Container Pattern

Components can contain children via the Container class:

```typescript
export class Container implements Component {
  children: Component[];

  addChild(component: Component): void;
  removeChild(component: Component): void;
  clear(): void;
  render(width: number): string[];
}
```

### TUI Layout

The TUI class manages the overall layout with two regions:

```
┌─────────────────────────────────────────┐
│ Scrollable Content                      │
│  - Messages                             │
│  - Thinking blocks                      │
│  - Tool executions                      │
│                                         │
│  ... (virtual scrolling)                │
├─────────────────────────────────────────┤
│ Fixed Footer                            │
│  - Editor input                         │
│  - Status bar                           │
│  - Notifications                        │
└─────────────────────────────────────────┘
```

```typescript
export class TUI extends Container {
  setFixedFooterStart(): void;  // Marks boundary
  scrollToBottom(): void;
  setFocus(component: Component | null): void;
}
```

### Differential Rendering

The TUI uses efficient redraw strategies:

1. **Synchronized Output:** Uses DECSC mode to prevent flicker
2. **Viewport Culling:** Only renders visible lines
3. **Scroll Detection:** Auto-scrolls unless user scrolled up
4. **Dirty Flag:** Queues renders, coalesces multiple requests

```typescript
private doRender(): void {
  // Clear scrollback, screen, home
  buffer.push("\x1b[3J\x1b[2J\x1b[H");

  // Apply scroll offset
  const visibleScrollable = scrollableLines.slice(
    this.scrollOffset,
    this.scrollOffset + scrollableViewport,
  );

  // Join with CR LF for synchronized mode
  buffer.push(visibleLines.join("\r\n"));
}
```

### Virtual Scrolling

Content beyond the viewport is virtualized:

```typescript
scrollOffset = Math.max(
  0,
  scrollableLines.length - scrollableViewport,
);
```

Mouse wheel events adjust the scroll offset:
- Button 64: Scroll up
- Button 65: Scroll down

### Modal System

Modals overlay the main content:

```typescript
export interface Modal extends Component {
  backdrop?: boolean;
  handleInput?(data: string): void;
}

showModal(modal: Modal): void;
hideModal(): void;
```

### Input Handling

Input is processed in stages:
1. Mouse events (SGR format)
2. Bracketed paste mode
3. Ctrl+Z (background)
4. Ctrl+C (exit)
5. Modal input
6. Focused component
7. Navigation keys

### Component Library

| Component | Purpose |
|-----------|---------|
| `Box` | Border decoration |
| `Text` | Static text display |
| `Markdown` | Formatted markdown rendering |
| `UserMessage` | User input display |
| `AssistantMessage` | AI response display |
| `ThinkingBlock` | Model reasoning display |
| `ToolExecution` | Tool call/result display |
| `Loader` | Spinner/activity indicator |
| `ProgressBar` | Progress indication |
| `Notification` | Toast messages |
| `Header` | Application header |
| `Footer` | Status bar |
| `Input` | Text input with editing |
| `SelectList` | Interactive selection |
| `Table` | Formatted table display |

## Consequences

### Positive
- Declarative component model is easy to reason about
- Differential rendering is efficient
- Container pattern enables composition
- Modal system allows focused interactions
- Virtual scrolling handles large histories

### Negative
- Custom rendering loop means reinventing some wheel
- Terminal escape sequences are platform-dependent
- No built-in animations or transitions

### Alternatives Considered

**blessed / blessed-contrib:** These libraries provide rich TUI components but have heavy dependencies and limited customization. Custom implementation is lighter. Rejected for control and simplicity.

**React for terminals (ink, react-blessed):** Would bring React patterns but adds complexity and bundle size. Rejected for simplicity.

**ncurses:** Standard library but complex C API. Not suitable for TypeScript. Rejected.

