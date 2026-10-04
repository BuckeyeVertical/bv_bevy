# Proving-ground runtime assets

Runtime assets for the `forest` world (`cargo run -- forest`).
Everything here is generated from the Poly Haven downloads by the scripts in
`tools/blender/`; the originals are extracted into `assets/raw/` (git-ignored,
~5 GB) and never modified.

| Folder | File | Source | Script |
|---|---|---|---|
| `vegetation/` | `trees.glb` — 17 pines/firs × LOD0–2 + LOD3 impostor | Pine Forest collection | `export_trees.py` |
| | `forest_props.glb` — ferns, mossy rocks, branches, stump, log, roots, moss | Pine Forest collection | `export_props.py forest_props` |
| | `grass.glb` — 11 grass clumps × 2 LODs | Grass Medium 01 | `export_props.py grass` |
| `buildings/` | `shed.glb` — shed + greenhouse | The Shed collection | `export_props.py shed` |
| `props/` | `shed_props.glb`, `barrel_01.glb`, `concrete_barrier.glb`, `electricity_poles.glb` | The Shed, Barrel 01, Concrete Road Barrier, Modular Electricity Poles | `export_props.py <job>` |
| `materials/` | `terrain_albedo/normal.jpg` (5-layer arrays), `asphalt_*`, `factory_wall_*` | grass004, withered_grass, Pine Forest ground textures, Gravel Road, Asphalt Floor, Factory Wall | `export_textures.py` |
| `sky/` | `meadow_2_cubemap.ktx2`, `meadow_2.json` (sun direction) | Meadow 2 HDRI | `hdri_to_cubemap.py` |

The terrain shader is `assets/forest/terrain.wgsl`.

## Rebuilding

```bash
# 1. extract downloads (once)
mkdir -p assets/raw && cd assets/raw
for f in ~/Downloads/assets/*.blend.zip; do n=$(basename "$f" .blend.zip); mkdir -p "$n"; unzip -oq "$f" -d "$n"; done
unzip -oq ~/Downloads/assets/pine_forest.zip -d pine_forest
unzip -oq ~/Downloads/assets/the_shed.zip -d .
mkdir -p sky && cp ~/Downloads/assets/meadow_2_4k.hdr sky/
cd ../..

# 2. convert (Blender 5.x, zstd and ffmpeg on PATH)
blender -b assets/raw/pine_forest/polyhaven_pine_fir_forest.blend --scene trees --python tools/blender/export_trees.py
blender -b assets/raw/pine_forest/polyhaven_pine_fir_forest.blend --python tools/blender/export_props.py -- forest_props
blender -b assets/raw/grass_medium_01_4k/grass_medium_01_4k.blend --python tools/blender/export_props.py -- grass
blender -b assets/raw/the_shed/the_shed.blend --python tools/blender/export_props.py -- shed
blender -b assets/raw/the_shed/the_shed.blend --python tools/blender/export_props.py -- shed_props
blender -b assets/raw/Barrel_01_4k/Barrel_01_4k.blend --python tools/blender/export_props.py -- barrel
blender -b assets/raw/concrete_road_barrier_4k/concrete_road_barrier_4k.blend --python tools/blender/export_props.py -- barrier
blender -b assets/raw/modular_electricity_poles_4k/modular_electricity_poles_4k.blend --python tools/blender/export_props.py -- poles
blender -b --python tools/blender/export_textures.py
blender -b --python tools/blender/hdri_to_cubemap.py -- assets/raw/sky/meadow_2_4k.hdr assets/forest/sky/meadow_2 --face 1024
```

## Notes on the conversions

- **Trees.** The Poly Haven trees are geometry-node trees that realise 2–7 M
  triangles each. `export_trees.py` renders every twig variant to an alpha card
  atlas, swaps the cards into the trees' twig collections so the node groups
  rebuild each tree from cards, decimates bark, and builds LODs
  (≈7–28k / 1–6k / 0.1–1.6k triangles) plus a crossed-billboard impostor.
- **Sky.** Meadow 2's photographed horizon trees (one reaches ~58°) would float
  above our forest, so the skybox is a smooth sky fitted to its clean sky texels
  with its measured sun halo; the sun disc is clamped out of the IBL and
  supplied by a directional light aligned with the HDRI's sun.
- **Textures** are downscaled to 512–2048 px. Bevy does not create mip chains
  for PNG/JPEG, so the environment generates them at load (with alpha-coverage
  preservation for foliage).
- **Scale.** 1 unit = 1 m. Ferns are scaled ×2.2 at export (scatter sources are
  authored small); poles are scaled ×1.38 at placement to ~8.5 m.
