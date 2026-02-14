# MCP Prompts

SYNAPSEED provides 6 prompt templates that guide the LLM through complex multi-step workflows using the available tools and resources.

## `describe_architecture`

Analyze and describe the project architecture using SYNAPSEED's semantic understanding.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `path` | No | Directory to analyze (default: project root) |
| `depth` | No | `overview`, `detailed` (default), or `deep` |

**Workflow:**
1. `hoist` to index the project
2. Read `synapseed://status` for project state
3. Read `synapseed://dna` for configuration
4. `lookup` for key types (detailed/deep modes)
5. `blame` on key files (deep mode only)

---

## `analyze_architecture`

Analyze project architecture health with structural metrics and recommendations.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `refresh` | No | `true` to force fresh analysis, `false` (default) to use cache |

**Workflow:**
1. `architect` to get health analysis
2. Read `synapseed://architect/health` for current state
3. `consult` for architecture guidance from DNA
4. Identify violations and coupling issues
5. Provide actionable recommendations

---

## `fix_build_errors`

Diagnose and fix current build errors using the shadow compiler.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `auto_fix` | No | `true` to auto-apply, `false` (default) to ask first |

**Workflow:**
1. Read `synapseed://diagnostics/active`
2. `diagnostics` for full error list
3. Group by file and severity
4. `quickfix` for `MachineApplicable` suggestions
5. Re-check diagnostics to verify

---

## `explain_evolution`

Analyze why code looks the way it does by tracing its git evolution.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `file` | Yes | File path to analyze |
| `start_line` | No | Start line of region |
| `end_line` | No | End line of region |

**Workflow:**
1. `hoist` for project structure
2. `lookup` for symbols in the file
3. `analyze` for churn and patterns
4. `blame` for detailed blame

**Output:** Timeline, complexity narrative, risk assessment, recommendations.

---

## `security_audit`

Perform a security audit of the project.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `scope` | No | `quick`, `standard` (default), or `full` |

**Workflow (standard):**
1. Read `synapseed://security/policy`
2. Read `synapseed://status`
3. `hoist` to find all files
4. `scan` on each config file
5. `check` on common project commands
6. `diagnose` for overall health

**Output:** DLP findings, command policy evaluation, risk assessment.

---

## `optimize_hotspots`

Analyze runtime performance hotspots from OTLP telemetry data.

**Arguments:**
| Name | Required | Description |
| :--- | :--- | :--- |
| `threshold` | No | Minimum avg_duration_ms to flag (default: 100) |

**Workflow:**
1. Read `synapseed://telemetry/hotspots`
2. Filter by threshold
3. `lookup` for each hot function
4. `analyze` for churn context
5. `diagnostics` for compiler warnings

**Output:** Hotspot report, optimization suggestions, priority-ordered action plan.
