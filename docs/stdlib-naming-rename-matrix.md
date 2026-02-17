# Stdlib Naming Compliance Rename Matrix (V1 Hard Rename)

This matrix captures the v1 breaking rename set applied to make stdlib naming comply with compiler rules.

## Data Modules

- `language/stdlib/data/list.wr`
  - `push` -> `add_to_list`

- `language/stdlib/data/parse.wr`
  - import/call updates: `push(...)` -> `add_to_list(...)`
  - boolean locals renamed to `is_*`/`has_*`
  - loop binder `_` -> `exponent_step`

## Runtime Modules

- `language/stdlib/runtime/scheduler.wr`
  - `scheduler_at_least` -> `clamp_scheduler_at_least`
  - `scheduler_clamp_integer` -> `clamp_scheduler_integer`
  - `scheduler_should_steal_work` -> `scheduler_is_ready_to_steal_work`
  - `deterministic_mode` -> `is_deterministic_mode`

- `language/stdlib/runtime/task.wr`
  - `task_at_least` -> `clamp_task_at_least`
  - `task_should_wake` -> `task_is_ready_to_wake`
  - `example_task_wake_flow` (`to -> Boolean`) -> `task_wake_flow_is_valid` (`check -> Boolean`)
  - `example_task_primitive_flow` (`to -> Boolean`) -> `task_primitive_flow_is_valid` (`check -> Boolean`)
  - collection locals renamed plural (`state` -> `task_states`, `action` -> `actions`)

- `language/stdlib/runtime/reactor.wr`
  - `reactor_at_least` -> `clamp_reactor_at_least`
  - `example_reactor_flow` (`to -> Boolean`) -> `reactor_flow_is_valid` (`check -> Boolean`)

- `language/stdlib/runtime/actor.wr`
  - `actor_at_least` -> `clamp_actor_at_least`
  - `objective_scale` -> `compute_objective_scale`
  - `paused_mailbox_should_drop_message` -> `mailbox_has_paused_drop_condition`
  - `fire_burst_begin` -> `start_fire_burst`
  - `fire_burst_end` -> `stop_fire_burst`
  - `fire_burst_abort` -> `stop_fire_burst_abort`

- `language/stdlib/runtime/pool.wr`
  - `auto_size` -> `compute_auto_size`
  - `pool_at_least` -> `clamp_pool_at_least`
  - `queue_should_drop_on_full` -> `queue_is_drop_on_full`

## Host Modules

- `language/stdlib/host/env.wr`
  - import/call updates: `push(...)` -> `add_to_list(...)`
  - boolean locals renamed to `is_*`/`has_*`
  - collection locals renamed pluralized forms
  - pattern binders `_` replaced with named binders

- `language/stdlib/host/log.wr`
  - `err` -> `emit_error`
  - `error_with` -> `emit_error_with`
  - `log_debug` -> `debug_log`
  - `log_info` -> `info_log`
  - `log_warn` -> `warn_log`
  - `log_warning` -> `warn_log_alias`
  - `log_error` -> `emit_error_log`
  - `log_debug_with` -> `debug_log_with`
  - `log_info_with` -> `info_log_with`
  - `log_warn_with` -> `warn_log_with`
  - `log_warning_with` -> `warn_log_with_alias`
  - `log_error_with` -> `emit_error_log_with`

## First-Party Call Site Updates

- `compiler/tests/codegen.rs`
  - `Logger.log_info(...)` -> `Logger.info_log(...)`
  - `Logger.log_warning(...)` -> `Logger.warn_log_alias(...)`
  - `Logger.log_error_with(...)` -> `Logger.emit_error_log_with(...)`
  - `scheduler_should_steal_work` -> `scheduler_is_ready_to_steal_work`
