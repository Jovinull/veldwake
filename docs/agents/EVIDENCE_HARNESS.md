# Evidence harness on the audited Windows host

Last updated: 2026-09-17

Headless tests prove logic. They cannot prove that the screen shows a continuous floor, that a window survives a minimize, or that a level transition is invisible to a person. Every milestone since M3B has needed driven evidence from a real window on the audited host, and the harness that produces it has been rebuilt from scratch more than once because only its conclusions were written down. This document records the procedure, the traps, and the validity rules so the next agent rebuilds it in minutes and, more importantly, does not trust an invalid run.

Read this together with [`../engineering/OBSERVABILITY.md`](../engineering/OBSERVABILITY.md), which says what the counters mean, and [`../engineering/PERFORMANCE.md`](../engineering/PERFORMANCE.md), which holds the recorded numbers.

## Three kinds of run, never mixed

| Run | Debug views | Screen capture | Produces |
|---|---|---|---|
| Benchmark | off | none | the counters quoted in milestone tables and `PERFORMANCE.md` |
| Driven smoke | any | yes | visual evidence, window lifecycle, exit code |
| Timing A/B | the thing being compared | none | frame-time comparisons between configurations |

Mixing them invalidates the result. `Graphics.CopyFromScreen` stalls the captured process: the same debug view measured 26.6 ms per frame in a clean run and 69.3 ms in a capture-heavy one. A benchmark with debug views on is not comparable to one with them off, which is why the views must stay free when off.

## Driving the client from PowerShell

Start the process yourself instead of using `Start-Process -PassThru`, which did not report the exit code reliably here:

- `System.Diagnostics.Process` from a `ProcessStartInfo` with `UseShellExecute = $false`, and **both** `RedirectStandardOutput` and `RedirectStandardError`, each read with `ReadToEndAsync` and joined at the end. `tracing_subscriber::fmt` writes to stdout; a harness that redirects only stderr captures an empty log.
- Set `RUST_LOG` and `VELDWAKE_PROFILE` through `$psi.EnvironmentVariables`.
- Wait about three seconds, then `$proc.Refresh(); $proc.MainWindowHandle`. A zero handle means the window never opened; abort instead of driving the desktop.

Input and window control come from `user32.dll` through `Add-Type`:

| Purpose | Call |
|---|---|
| Key down / up | `keybd_event(vk, MapVirtualKey(vk, 0), 0 or 2, 0)` |
| Mouse look | `mouse_event(0x0008)`, then repeated `mouse_event(0x0001, dx, dy)` about 16 ms apart, then `mouse_event(0x0010)` |
| Client area for capture | `GetClientRect` plus `ClientToScreen` |
| Window geometry | `MoveWindow` |
| Minimize / restore | `ShowWindow(h, 6)` / `ShowWindow(h, 9)` |
| Focus | `SetForegroundWindow`, verified with `GetForegroundWindow`, plus `IsIconic` |

Virtual keys in use: `W 0x57`, `A 0x41`, `S 0x53`, `D 0x44`, `Escape 0x1B`, `F1 0x70`, `F2 0x71`, `Alt 0x12`.

`winit` accepts injected raw mouse motion, so scripted look works without a real device.

## Focus discipline is not optional

Injected keys reach only the foreground window. Every input must be preceded by a focus routine that restores the window if `IsIconic`, taps `Alt` down and up (the documented unlock for `SetForegroundWindow`), calls `SetForegroundWindow`, sleeps briefly, and verifies with `GetForegroundWindow`. Retry about ten times, then **abort the run and mark it invalid**. A run that silently lost focus looks complete and is worthless: the camera never moved, so every counter describes a stationary client.

## The canonical traversal path

Every profile must run the identical path or the comparison means nothing:

1. settle 12 s
2. pitch down (30 mouse steps of `dy = 5`)
3. `D` 8 s, `D` 8 s
4. `A` 16 s, `A` 12 s
5. `W` 10 s
6. `S` 16 s
7. `D` 12 s (re-entry)
8. rest 6 s

At 12 world units per second this crosses the `Lod0`/`Lod1` band in both signs on `x` and `z`, leaves the finite source's extent, and re-enters it.

## Validity checks before trusting any run

- **`cpu_evictions = 0` on a traversal path means the client never moved.** This is the reliable tell-tale of a focus failure. Discard the run.
- The `camera_chunk` sequence in the log must show movement in both signs on `x` and `z`.
- The exit code must be `0` and the log must contain no validation error.
- Two profiles being compared should report similar `cpu_evictions` and `loads_dispatched`; that is the evidence they really walked the same path.
- A capture whose pixels include another window (a Start menu opening over the client, an editor behind a transparent title bar) is contaminated. Say so and exclude it rather than quietly reusing it.

## Capture modes

- **At rest.** Two captures three seconds apart must be identical. This proves no hole persists, and nothing else.
- **Mid-movement.** Capture while the key is still held, at roughly 40% and 80% of the leg. A hole that exists only while streaming is running is invisible at rest, and resting captures were the reason a four-second band of missing floor went unnoticed.
- **Burst plus contact sheet.** When a hole is suspected, capture every 300 ms for a whole leg and assemble the frames into one labeled grid image with `System.Drawing` (six columns, index drawn on each tile). Reading twenty tiles at once localizes the frame where geometry disappears and the frame where it returns. This is how frontier starvation was found after the gap counters read zero.

## Timing A/B

Idle camera, no captures, one fixed window per configuration (12 s is enough for two five-second report intervals), switching configuration with a single key tap between windows. Report the per-interval `average_wall_frame_ms` and `observed_fps`, and note that the first interval after a switch spans the transition and should not be quoted alone.

## Where output lands

Under `%TEMP%\veldwake-*`: `client.log` (stdout and stderr joined), `marks.txt` (timestamped script actions, which is how a screenshot is aligned with a log interval), and the PNGs. Keep the marks file; correlating a capture with the interval line that covers it is otherwise guesswork.

## What this harness encodes

Four conclusions that cost real time to reach, kept here because they are about producing evidence rather than about any one milestone:

- **Counters and pixels answer different questions and neither substitutes for the other.** A zero gap counter has coexisted with four seconds of missing floor, and a non-zero ready-but-undrawn counter has coexisted with a perfectly continuous one. Decide in advance which counter would have to move for a visual claim to be true, then look at the capture anyway.
- **The harness is part of the evidence, not a scratch detail.** Three milestones in a row rebuilt the same Win32-driven client because only its conclusions were written down.
- **The cheapest validity check is a counter that only moves when the camera moves.** `cpu_evictions = 0` on a traversal path means focus was lost and the whole run is fiction; it is far more reliable than watching the window.
- **Record the formula next to any derived number.** Total measured snapshot-plus-worker CPU time is `snapshot_build + worker_mesh_lod0 + worker_mesh_lod1`. `lod1_derivation` is already a subset of `snapshot_build`; report it separately and never add it a second time. Without the formula the next run's comparison silently changes definition.

## Why the scripts are not in the repository

The harness is operator tooling for one host: Windows-only PowerShell with Win32 interop, useful to an agent working on the audited machine and to nobody else. It has been kept in the agent's scratchpad so far rather than committed, because adding it would create a second, unreviewed surface that ages independently of the client it drives. Everything needed to rebuild it is above. Committing it is a reasonable owner decision, not an agent one; if it ever happens, it carries the same review and documentation obligations as code.
