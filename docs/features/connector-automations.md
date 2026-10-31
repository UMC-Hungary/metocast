# Connector automations

Rules that run inside the core, so they work with no UI open: _when_ one device does something
(a Middle Control preset finishes moving, a RØDECaster channel goes live or silent), Metocast
_does_ something on others (move a camera, start or stop recorders). A rule fires when any of its
triggers happens and runs all of its actions, in order.

The trigger and action kinds, the options and how a run behaves are in the
[README](../../README.md#connector-automations).

## Use it

**Equipment → Automations** in the sidebar (**Settings → Equipment** on the iPhone) in the Apple app. Create a rule, give it triggers and
actions, and arm Automations. **Run now** runs a rule's actions at once, ignoring the cooldown and
the armed switch, to check a setup; it really records and really moves cameras.

## Entry points

| Does                          | REST                             | WebSocket               | MCP tool                |
| ----------------------------- | -------------------------------- | ----------------------- | ----------------------- |
| List the rules and the switch | `GET /api/automations`           | `automations.list`      | `automations_list`      |
| Create a rule                 | `POST /api/automations`          |                         | `automations_create`    |
| Replace a rule                | `PUT /api/automations/{id}`      |                         | `automations_replace`   |
| Delete a rule                 | `DELETE /api/automations/{id}`   |                         | `automations_delete`    |
| Arm or disarm everything      | `PUT /api/automations/armed`     | `automations.set_armed` | `automations_set_armed` |
| Run a rule now                | `POST /api/automations/{id}/run` | `automations.run`       | `automations_run`       |
| Recent runs                   | `GET /api/automations/runs`      |                         | `automations_runs`      |

Pushes: `automations.changed` when the rules or the switch change, `automation.run` for each run,
and a `notification` at level `warn` when a run had a failure.

`automations_run` really starts recorders and moves cameras, so it is not a read-only tool.
