# pure-plan-mode

Plan mode as a user-triggered feature, not an always-loaded model tool.

- `/plan` (or the composer's **Plan** toggle) turns plan mode on: the engine
  injects read-only planning guidance and this plugin activates `exit_plan`.
- The model investigates, then calls `exit_plan` once with the whole plan. The
  call asks the user to approve, revise or reject it.
- Approving (or rejecting) restores the tool set; revising keeps plan mode on.
- While plan mode is off, `exit_plan` is removed from the active tool set, so
  the model never sees it in its context.
