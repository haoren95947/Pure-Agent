# pure-swarm

Swarm batches as a user-triggered feature, not an always-loaded model tool.

- `/swarm on` (or the composer's **Swarm** toggle) activates the `swarm` tool
  and appends the swarm guidance to the session context.
- The model can then call `swarm` once with a prompt template and one item per
  child; the tool starts one host-managed child per item, waits for all of
  them, and returns one aggregated report.
- `/swarm off` removes the tool from the active set again, so the model never
  sees it unless the user asked for a swarm.

While swarm mode is off the tool is unloaded from context by the plugin's
`SessionStart` handler.
