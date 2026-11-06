# Generated material

*What generative models made for wrela, where it came from and what it fed. The rule is in [citations.md](citations.md): models make targets and sources, and only wrela code ships.*

## The log

Every output a model makes for wrela gets one line in [generated.jsonl](generated.jsonl), in the change that first uses it, or the same day for targets. That includes targets only judged against, and outputs thrown away. A converted piece's code also names its log line in a comment.

The log records provenance: who made what, with which model, under which terms. It doesn't make a model's output ours. Under US law an output with no human authorship can't be copyrighted (*Thaler v. Perlmutter*, cert. denied 2 March 2026). What we own is the wrela code a person directed and agents wrote from it. So the log also says which code each output fed.

Each line is one JSON object:

| Field | What it holds |
|---|---|
| `date` | The day it was made (UTC) |
| `host` | Where it ran: `replicate`, `local`, a vendor's API |
| `model` | The model as the host names it, such as `black-forest-labs/flux-2-pro` |
| `version` | The host's version id; `hidden` when the host doesn't expose it |
| `prediction` | The host's run id, if any |
| `prompt` | The prompt as sent |
| `inputs` | Every other input; files by their path and, where kept, sha256 |
| `seed` | The seed, from the input or the run's log; `null` if the host randomised it and didn't report it |
| `licence` | A key into the table below |
| `output` | `file` (where it is kept) and `sha256` |
| `use` | `target` (judged against), `source` (converted into code), or `discarded` |
| `fed` | The wrela files built from it; empty for a target |
| `note` | Anything else: a redo, a defect, a log written after the fact |

Targets stay outside the repo, as style references do. The project keeps them in its shared files (`genai-targets/`, `genai-proofs/`); the log keeps their hashes.

## Models and their terms

Use a model only if its terms allow commercial use of its outputs worldwide, with no territory or user-count limit. Add a row before a model's first use.

| Key | Model | Terms | Outputs usable commercially |
|---|---|---|---|
| `flux-2-pro-api` | FLUX.2 [pro], Black Forest Labs, through its paid API on Replicate | [BFL Developer Terms](https://bfl.ai/legal/developer-terms-of-service) and [FLUX API Service Terms](https://bfl.ai/legal/flux-api-service-terms), revised 2026-08-04: we own the output; BFL may train on inputs and outputs. Outputs may not train, distil or fine-tune another AI model, and may not be presented as made by a person. | Yes, for targets. Don't train a network (such as an `engine::neural` fit) directly on a FLUX image. |
| `trellis-mit` | TRELLIS, Microsoft, through Replicate (`firtoz/trellis`) | [MIT](https://github.com/microsoft/TRELLIS/blob/main/LICENSE) | Yes |
| `kimodo` | Kimodo-SOMA-RP-v1.1, NVIDIA, through Replicate (`sprited/kimodo`) | [NVIDIA Open Model License](https://huggingface.co/nvidia/Kimodo-SOMA-RP-v1.1): NVIDIA claims no ownership of outputs and doesn't limit distributing them | Yes; training data under legal review before launch |

Not allowed:

| Model | Why |
|---|---|
| FLUX.2 [dev] and FLUX.1 [dev] weights, run ourselves or through a host that runs them | [FLUX Non-Commercial License v2.1](https://huggingface.co/black-forest-labs/FLUX.2-dev/blob/main/LICENSE.md): its outputs clause allows commercial use, but other clauses bar any commercial or production use of data the model produces |
| Hunyuan3D 2.x (Tencent) | Its licence excludes the EU, UK and South Korea and caps monthly users |
| Video-to-motion built on SMPL (GVHMR, WHAM and the like) | The SMPL body model is non-commercial |
| Meta AudioGen and MusicGen weights | CC BY-NC |
| Suno, Udio | Ongoing label lawsuits; Udio no longer allows downloads |
