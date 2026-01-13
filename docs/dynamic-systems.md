# Dynamic Systems Design Notes

This note captures design considerations for dynamic systems and queries in Dimensify,
with a comparison between a custom dynamic-system runner (QueryBuilder + SystemParamBuilder)
and the bevy_mod_scripting reflection-based approach. It also outlines how built-in
Bevy types (e.g., Transform) are accessed and what future direction seems most promising.

## Goals and context

- Build and run systems dynamically at runtime.
- Queries are defined at runtime (no Rust types available).
- Components may be new (defined in Python) or existing Bevy components.
- Minimize overhead while keeping an ergonomic API for a Python client.

## Terminology

- "Dynamic system" means systems assembled at runtime from query specs.
- "Dynamic component" means components registered at runtime without a Rust type.
- "Built-in component" means a Bevy component type with a Rust definition (e.g., Transform).

## Comparison: custom dynamic system vs bevy_mod_scripting

| Topic | Custom dynamic system (QueryBuilder + SystemParamBuilder) | bevy_mod_scripting (reflection) |
| --- | --- | --- |
| Query build | Build QueryState from QueryBuilder using ComponentId access specs. | Build QueryState from QueryBuilder using ScriptComponentRegistration. |
| Param wiring | Use SystemParamBuilder to assemble a system with dynamic params. | Build DynamicScriptSystem with custom initialization. |
| Runtime access | Direct pointer access or manual layout offsets for fields. | ReflectReference access via bevy_reflect. |
| Component types | Works for runtime-defined components if layout is known. | Works for runtime-defined components if Reflect is available. |
| Ergonomics | More code to write, full control over data shape. | Higher-level API, less glue code. |
| Perf profile | Fast if direct access is used, overhead similar to Bevy queries. | Slower due to reflection/dynamic dispatch. |

## How built-in components are accessed (e.g., Transform)

### Custom dynamic system

Options for built-in types:

1. **Direct layout access**:
   - Use ComponentId + known Rust layout to read/write raw memory.
   - Requires stable layout knowledge and careful field offsets.
   - Fastest but brittle and unsafe if layout changes.

2. **Reflect-based access**:
   - Use AppTypeRegistry + ReflectComponent to get a Reflect reference.
   - Use Reflect to read/write fields by name (e.g., "translation", "rotation").
   - Slower than raw access but avoids layout coupling.

For Transform specifically, a reflective read/write can access "translation" and then
update the Vec3 fields (x/y/z) via Reflect or by converting to a typed Transform
if allowed.

### bevy_mod_scripting

bevy_mod_scripting already uses reflection for all component access.

- Built-in types like Transform are accessed via ReflectReference.
- Field access uses reflection paths (e.g., "translation.x") under the hood.
- No Rust type is required at runtime, only registration in the type registry.

## Runtime-defined components (no Rust type)

Both approaches need a contract for data shape:

- **Custom dynamic system**: register a ComponentDescriptor with a known layout
  (for example, a fixed-size array or packed struct). The Python client must send
  shape + field layout info.
- **bevy_mod_scripting**: create a DynamicComponent or a Reflect-based dynamic type
  with field metadata. Access is through reflection by field name.

## Performance considerations

1. **Query iteration cost** is similar in both approaches (normal Bevy query cost).
2. **Component read/write cost** is the differentiator:
   - Custom: raw pointer access is fastest.
   - mod_scripting: reflection is slower due to dynamic lookup and dispatch.
3. **Overhead**:
   - Custom: lower, but requires more glue code.
   - mod_scripting: higher, but much more ergonomic out of the box.

## Future direction notes

- **Two-path access**: provide a fast POD/columnar path for JIT-like compute,
  with a reflection fallback for arbitrary components.
- **Stable type registry**: use type-path strings from Python and resolve to
  ComponentId in Rust (ComponentId is not stable across runs).
- **Schema cache**: cache component layouts and query states to avoid rebuild cost.
- **Explicit access declarations**: keep access specs (read/mut/with/without)
  in a small schema for scheduling and safety checks.

## Suggested next steps

- Define a minimal "DynamicSystemRunner" in Dimensify that:
  - Takes query specs by type-path.
  - Builds QueryStates via QueryBuilder.
  - Exposes components via either reflection or raw layout access.
- Document the component schema contract required for raw access.
- Decide if the Python client should always declare field layouts or
  if Reflect should be the default for built-in and dynamic types.
