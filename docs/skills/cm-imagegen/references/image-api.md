# Image API quick reference

This file is for the fallback CLI mode only. Use it only after the user explicitly asks to use `scripts/image_gen.py` instead of the built-in `image_gen` tool.

These parameters describe the Image API and bundled CLI fallback surface. Do not assume they are normal arguments on the built-in `image_gen` tool.

## Scope
- This fallback CLI is intended for GPT Image models (`gpt-image-1.5`, `gpt-image-1`, and `gpt-image-1-mini`).
- The built-in `image_gen` tool and the fallback CLI do not expose the same controls.

## Endpoints
- Generate: `POST /v1/images/generations` (`client.images.generate(...)`)
- Edit: `POST /v1/images/edits` (`client.images.edit(...)`)

## Core parameters for GPT Image models
- `prompt`: text prompt
- `model`: image model
- `n`: number of images (1-10)
- `size`: `1024x1024`, `1536x1024`, `1024x1536`, or `auto`
- `quality`: `low`, `medium`, `high`, or `auto`
- `background`: output transparency behavior (`transparent`, `opaque`, or `auto`) for generated output; this is not the same thing as the prompt's visual scene/backdrop
- `output_format`: `png` (default), `jpeg`, `webp`
- `output_compression`: 0-100 (jpeg/webp only)
- `moderation`: `auto` (default) or `low`

## Edit-specific parameters
- `image`: one or more input images. For GPT Image models, you can provide up to 16 images.
- `mask`: optional mask image
- `input_fidelity`: `low` (default) or `high`

Model-specific note for `input_fidelity`:
- `gpt-image-1` and `gpt-image-1-mini` preserve all input images, but the first image gets richer textures and finer details.
- `gpt-image-1.5` preserves the first 5 input images with higher fidelity.

## Output
- `data[]` list with `b64_json` per image
- The bundled `scripts/image_gen.py` CLI decodes `b64_json` and writes output files for you.

## Limits and notes
- Input images and masks must be under 50MB.
- Use the edits endpoint when the user requests changes to an existing image.
- Masking is prompt-guided; exact shapes are not guaranteed.
- Large sizes and high quality increase latency and cost.
- High `input_fidelity` can materially increase input token usage.
- If a request fails because a specific option is unsupported by the selected GPT Image model, retry manually without that option.

## Image generation API (provider-configured)

When the active `model_provider.base_url` points at the image_generation API (hostnames `minimax.io` or `minimaxi.com`), the CLI routes `generate` and `edit` to `POST /v1/image_generation` instead of the OpenAI Images endpoints.

### Endpoint
- Generate / edit: `POST /v1/image_generation`

### Core request fields
- `model`: image model (default `image-01`)
- `prompt`: text prompt (max 1500 characters)
- `aspect_ratio`: `1:1` (default), `16:9`, `4:3`, `3:2`, `2:3`, `3:4`, `9:16`, `21:9`. Takes priority over `width`/`height`.
- `width` / `height`: pixels, range [512, 2048], must be divisible by 8, and must be set together.
- `response_format`: `url` (default, link expires in 24 hours) or `base64`
- `seed`: random seed for reproducibility
- `n`: number of images, range [1, 9], default 1
- `prompt_optimizer`: boolean, default `false`

### Image-to-image (edit)
- `subject_reference`: array of `{ "type": "character", "image_file": "<public URL or data:image/...;base64,...>" }`. For local files the CLI embeds each `--image` as a base64 data URL.

### Response
- `data.image_urls`: array of image URL strings (returned when `response_format=url`)
- `data.image_base64`: array of base64 image strings (returned when `response_format=base64`)
- `metadata.success_count` / `metadata.failed_count`
- `base_resp.status_code`: `0` on success; non-zero indicates failure (for example `1004` auth failed, `1008` insufficient balance, `1026` sensitive content).

The CLI downloads `url` responses and decodes `base64` responses, then writes PNG/JPG/WEBP files to the output directory.

## Important boundary
- `quality`, `input_fidelity`, explicit masks, `background`, `output_format`, and related parameters are OpenAI-Images fallback-only execution controls.
- They are not accepted by the image_generation API and are ignored when the provider targets that API.
- Do not assume they are built-in `image_gen` tool arguments.
