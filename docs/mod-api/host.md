# host

Interface `t-rustable:api/host@0.1.0`. No permission.

| Function | Notes |
|---|---|
| `log(level, message)` | Writes to the local log of T-Rustable. |
| `progress(fraction, message)` | `fraction` from 0.0 to 1.0. The GUI shows a progress bar. The CLI prints it. |
| `is-cancelled()` | True when the user cancelled the current call. Check it in long loops, then return the error `cancelled`. |
| `setting-get(key)` | The value, or none. |
| `setting-set(key, value)` | Error `limit-exceeded` when all settings of this mod pass 64 KB. |
| `setting-remove(key)` | |

Settings belong to one mod. Other mods cannot read them. Keep state that must survive in settings, not in mod memory: the host throws away the mod instance after a failure.
