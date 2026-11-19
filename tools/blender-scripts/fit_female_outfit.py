"""Fit one part of a segmented Tripo female outfit onto the female base rig.

blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_outfit.py -- --outfit ranger --part gloves [--keep-pending]
"""
import copy
import hashlib
import json
import subprocess
import sys
from datetime import date
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_outfit_adjust import TO_GLTF, bake, sleeve_end, sleeve_end_total
from outfits.female_outfit_pose import ArmPose, rotation_between, smoothstep
from outfits.mesh_components import edge_component_sizes, welded_islands
from outfits.skinning import deform, deform_dense

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
RIG_ID = 'human_female_01_local_v1'
ARMS = ('LeftArm', 'LeftForeArm', 'RightArm', 'RightForeArm')
LEGS = ('LeftLeg', 'LeftFoot', 'LeftToe', 'RightLeg', 'RightFoot', 'RightToe')
FEET = ('LeftFoot', 'LeftToe', 'RightFoot', 'RightToe')
CEREMONY_V2 = dict(source='tripo_multiview_v2/segmented.glb',
                   sha256='c1f2f134b29268b6671362923eb4114d6965fedff6495dd130b969a4691d9b90', scale=1)
# Leg skin of the female base, sRGB.
LEG_SKIN = (.767, .594, .494)
OUTFITS = {
    'priest': dict(
        source='tripo_segmented_v1/segmented.glb',
        sha256='2baf723d6fad2117159e6e369032711808f7a9a17b95b7819ffefbca7ae21bfe',
        scale=1.8, on_heels=False,
        parts={
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand')),
            'top': dict(nodes=['top', 'tripo_part_25_top_fragment'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        # Coverage hid neck skin under the open collar, leaving a hole; restore skin seen from outside.
                        restore_visible=True, cut_inside_glove=True, align_sleeves=True, hem_band=(.95, 1.15)),
            'bottom': dict(nodes=['skirt'], version='fitted_v1'),
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS),
            'helmet': dict(nodes=['helmet'], version='fitted_v1'),
        }),
    # Generated from a concept painted over the base in its 30° heel pose, so every part stands that much lifted.
    'ranger': dict(
        source='tripo_segmented_v1/original/model.glb',
        sha256='255e4ab93e253c9b63d1991600cb82973db8a0efaf7264ff366947df0a824c5f',
        # Tripo centred the model on its bounding box, which put the body 7 cm forward (head 8.3 cm, thighs 7.1 cm).
        scale=1.86, on_heels=True, offset_y=.07, snug_cover=True,
        parts={
            # Fingerless: the generated fingers are dropped and the base fingers stay visible.
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', drop_skin=True, hide_inside_cuff=True,
                           restore_visible=True),
            # The sleeve ends at the elbow, bare once the gloves come off, so it is aligned as one tube
            # and cut above its skin-coloured end band.
            'top': dict(nodes=['top'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        restore_visible=True, cut_inside_glove=True, align_sleeves='rigid', cut_skin_band=True),
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.12)),
            # Below the knee: the shin above the boot rim stays bare.
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=FEET),
            'hair': dict(nodes=['hair'], version='fitted_v1', restore_visible=True),
        }),
    # Same heel-pose concept as the ranger; the top and skirt kept Tripo's segment names.
    'bard': dict(
        source='tripo_segmented_v1/original/model.glb',
        sha256='f256e239c69c6924c3e418a97c92f692c5ebe9f93b963c79750bc30807e5984d',
        scale=1.86, on_heels=True, snug_cover=True,
        rename={'tripo_part_0': 'top', 'tripo_part_2': 'skirt'},
        parts={
            # Tripo's fingers are kept: dropping them for the base fingers left shards and empty holes.
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand'),
                           hide_inside_cuff=True),
            # The puffed sleeve is gathered into the bracer, so it is cut inside the glove cuff.
            'top': dict(nodes=['top'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        restore_visible=True, cut_inside_glove=True, align_sleeves='rigid'),
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.12)),
            # Generated with the hat over a bob, from a model whose body is about 7% shorter in Tripo units;
            # lowered so the brim sits just above the brows as in the concept.
            'helmet': dict(nodes=['hat'], version='fitted_v1',
                           model=dict(source='tripo_hat_v1/original/model.glb',
                                      sha256='cf126826dc4e85d8f13f790edefaf699e22efbdc9934f1bfec44e36264bbcc25',
                                      scale=2.0, offset_z=-.045)),
            # Tripo made the stiletto 2.5 cm short of the sole plane. The idle toe bend left the extra gaps
            # (metres, measured in game) after stretching to the heel-pose sole. The full stretch looked too long,
            # so only half of it is used (user decision 2026-11-05).
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=FEET,
                          extend_heel={'left': .0029, 'right': .0048, 'share': .5, 'upright': .09, 'lean': .005},
                          sole_colour=(.72, .55, .22)),
        }),
    # Generated on a blue mannequin of the heel-posed body and aligned to it by tools/split-female-ceremony-outfit.py.
    # The veil hid the bodice back in v1, so the bodice comes from v2, generated without the veil. v2's skirt
    # came out fused to the stockings with scraps under the hem, so the skirt stays v1's; the v2 bodice ends 9 mm
    # below the v1 skirt's top edge and tucks under its belt.
    'ceremony': dict(
        source='tripo_multiview_v1/segmented.glb',
        sha256='8aeecaa1c8e03c08c7529a7f72c1d5a443c0fb83c0d165f11f19ca21fc5fea58',
        scale=1, on_heels=True, snug_cover=True,
        parts={
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand'),
                           # Cut at the rim itself: skin left inside it showed through a dip in the gold band.
                           clip_cuff=0),
            # Off the shoulder: the arms stay bare between the sleeve bands and the gloves, and the bra straps go too.
            'top': dict(nodes=['top'], version='fitted_v1', hide_underwear=(1.15, 1.7), restore_visible=True,
                        clip_hem=dict(keep='below', within=.25), clip_edge=dict(above=1.3, within=.14),
                        model=CEREMONY_V2),
            # A petticoat bell: it hangs from the hips and its hem lifts only to clear the legs.
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.12),
                           robe=dict(waist_height_m=1.2, hem_height_m=.43, center_z_m=-.02, waist_radii_m=[.14, .11],
                                     driven_below_m=1.19, blend_m=.08, lift=1, max_angle=1.2,
                                     colliders=[dict(bone=side + bone, center=[sign * x, y, z], radii=radii)
                                                for side, sign in (('Left', 1), ('Right', -1))
                                                for bone, (x, y, z), radii in (
                                                    ('UpLeg', (.137, .785, -.04), [.095, .225, .105]),
                                                    ('Leg', (.2, .34, -.045), [.075, .265, .085]))])),
            # Thigh-high stockings and pumps in one piece.
            # The pump's sole was generated 1.55 cm above the plate boots' sole, whose lift every heel shares, and its
            # heel tip another 1.7 cm short: the shoe is lowered as one piece and both heels stretched by the same
            # 0.85 cm, half of that, so the heel does not land before the ball of the foot (user decision 2026-11-14).
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS, rigid_post=.075,
                          lower_shoe=.0155, extend_heel={'gap': .0085}, clip_hem=dict(keep='above', within=.4)),
            'helmet': dict(nodes=['helmet'], version='fitted_v1'),
        }),
    # Rica's working clothes, generated on the blue mannequin from front and back views and aligned by
    # tools/split-female-shopkeeper-outfit.py.
    'shopkeeper': dict(
        source='tripo_multiview_v2/segmented.glb',
        sha256='a5c222e0b9904a786ed2f00d1a27bf2f7ef9f4a8f4eeec2f58f3e22780f89e78',
        scale=1, on_heels=True, snug_cover=True,
        parts={
            # Blouse and corset. The sleeves end below the elbow and the forearms stay bare.
            'top': dict(nodes=['top'], version='fitted_v1', hide_underwear=(1.15, 1.7), restore_visible=True,
                        clip_hem=dict(keep='below', within=.25), clip_sleeve=dict(inset=.01)),
            # Belt, apron, pouch, keys and an ankle-length skirt on the robe rig. The apron hangs close enough for
            # the thighs to come through it, so the legs inside are hidden.
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.2),
                           hide_bones=('LeftUpLeg', 'LeftLeg', 'RightUpLeg', 'RightLeg'),
                           # No colliders: the rest shape of a skirt this narrow already touches the legs, and the
                           # rig keeps rest contact, so the hem would not lift. Each hem column follows its thigh.
                           robe=dict(waist_height_m=1.12, hem_height_m=.19, center_z_m=-.02, waist_radii_m=[.14, .11],
                                     driven_below_m=1.05, blend_m=.08, lift=1, max_angle=1.2)),
            # After the preview fit the sole sits 1.8 mm below the plate boots' sole, whose lift every heel shares:
            # the shoe is raised as one piece. The heel tip is 7 mm short and stretched by half, as on the
            # ceremony pumps.
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=FEET, rigid_post=.075,
                          lower_shoe=-.0018, extend_heel={'gap': .0036},
                          # Without the skirt the boot top shows: the shin is cut along it.
                          clip_hem=dict(keep='above', within=.4)),
        }),
    # The women's cut of the guard outfit, generated on the blue mannequin from front and back views and aligned by
    # tools/split-female-guard.py; the helmet came from its own bust and tools/split-male-guard-helmet.py --female.
    'guard': dict(
        source='tripo_body_v1/segmented.glb',
        sha256='dcaf1c2755efba1339cb26d25284909493c4400174e578b7576de0c5656c387a',
        # The mail's hips stand as far out as the arms and would pass for sleeve.
        scale=1, on_heels=True, snug_cover=True, arm_reach=.12,
        parts={
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand'),
                           clip_cuff=0),
            # Mail hauberk with its collar and belt; the sleeves run to the wrists under the glove cuffs.
            'top': dict(nodes=['top'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.7),
                        restore_visible=True, cut_inside_glove=True, align_sleeves=True),
            # Generated alone from the waistband down by tools/split-female-guard-skirt.py: on the body its straps
            # came fused to the mail's hem, which left a band 9 cm tall.
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.2),
                           model=dict(source='tripo_skirt_v1/segmented.glb',
                                      sha256='97e1a576381bfe6d1f46efbc750ac8e97693a640f59160c6c302d930a2907602', scale=1)),
            # Thigh-high stiletto boots with the knee cops strapped over them.
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS, rigid_post=.075,
                          clip_hem=dict(keep='above', within=.4)),
            'helmet': dict(nodes=['helmet'], version='fitted_v1',
                           model=dict(source='tripo_helmet_v1/segmented.glb',
                                      sha256='0aa366b476346266fbacdcb9783815dfa6cdf154725a3b673070b88ab2eaf4c6', scale=1)),
        }),
    # Miriel's maid uniform, generated on the blue mannequin from front and back views and aligned by
    # tools/split-female-maid-outfit.py.
    'maid': dict(
        source='tripo_multiview_v1/segmented.glb',
        sha256='779d0f2aadbbca5f4b7cadca90b4dcb8ae880b0f0b25a8de03dad0d02aeeb17a',
        # The gloves run past the elbow: placed by the hand alone their tubes stood 2 cm behind it.
        scale=1, on_heels=True, snug_cover=True, fit_arm=True,
        parts={
            # Satin gloves to above the elbow, each topped with a frill and a bow. They cover the whole forearm,
            # whose skin came through at the elbow in the idle pose. The frill stands off the arm, and the skin
            # hidden near it by whole triangles left notches above it: the arm is cut 1 cm inside the rim instead.
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1',
                           hide_bones=('LeftHand', 'RightHand', 'LeftForeArm', 'RightForeArm'),
                           clip_cuff=.01, cuff_past_elbow=True, align_ends=True),
            # Bodice with collar, bib, puff sleeves and corset waist; the arms stay bare below the sleeve cuffs.
            # The corset is drawn 1.5 cm tighter than the body's waist, so the skin that stands out of it is not
            # restored: it showed as a band of belly between corset and waistband.
            # Generated alone on a bust by tools/split-female-maid-top.py: the bodice that came with the whole
            # outfit had its collar at the trapezius, narrower than the neck there.
            'top': dict(nodes=['top'], version='fitted_v1', hide_underwear=(1.15, 1.7),
                        clip_hem=dict(keep='below', within=.25), clip_sleeve=dict(frill=-.012), align_sleeves='rigid',
                        sleeve_pivot='shoulder', sleeve_end=dict(start=.03, radius=.12, fade=.03),
                        rigid_collar=1.535,
                        hide_neck=dict(above=1.47, collar=1.545, level=.62, within=.1),
                        model=dict(source='tripo_top_v2/segmented.glb',
                                   sha256='6c20ac95d269b96c79ea33a892e036d9154a98c61b517a443d6c14ac1eb868f9', scale=1)),
            # Short skirt over a petticoat with the apron, its waistband and the back bow, hung from the hips as a
            # bell: skinned to the thighs its hem tore apart between the legs at a walk.
            # The waistband stood 1.7 cm behind the body's waist: the skirt moves forward as one piece.
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.25),
                           clip_hem=dict(keep='above', within=.25, shift=.01), model=dict(offset_y=-.017),
                           drop_lining=dict(near=.025, below=1.05, layer=500, scraps=1.15, scrap_faces=20),
                           robe=dict(waist_height_m=1.18, hem_height_m=.81, center_z_m=-.02, waist_radii_m=[.14, .11],
                                     driven_below_m=1.13, blend_m=.08, lift=1, max_angle=1.2,
                                     colliders=[dict(bone=side + bone, center=[sign * x, y, z], radii=radii)
                                                for side, sign in (('Left', 1), ('Right', -1))
                                                for bone, (x, y, z), radii in (
                                                    ('UpLeg', (.137, .785, -.04), [.095, .225, .105]),
                                                    ('Leg', (.2, .34, -.045), [.075, .265, .085]))])),
            # Thigh-high stockings and pumps in one piece. The sole came 1 cm below the plate boots' sole, whose lift
            # every heel shares, and the heel tip 2.5 cm short of it: the shoe is raised as one piece and the heel
            # stretched by half the gap, as on the ceremony pumps.
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS, rigid_post=.075,
                          lower_shoe=-.0102, extend_heel={'gap': .0127}, level_tops=True,
                          clip_hem=dict(keep='above', within=.4, per_side=True, shift=-.003),
                          # Leg skin (sRGB) blended into the black knit: opaque black read as leggings.
                          repaint=dict(mode='sheer', skin=LEG_SKIN, amount=.3, band=.078),
                          taper=dict(opening=.03, ankle=.07, ankle_fade=.05, clearance=.002, reach=.13)),
            'helmet': dict(nodes=['helmet'], version='fitted_v1'),
        }),
}
# Cocoly's pink maid outfit wears the maid's gloves and stockings in white.
MAID = OUTFITS['maid']
OUTFITS['pink_maid'] = dict(MAID, source='../female_maid_outfit/' + MAID['source'], parts={
    'gloves': dict(MAID['parts']['gloves'], repaint=dict(mode='white_cloth', white=(1, .985, .97), margin=8)),
    # White stockings and black pumps generated alone on the bare mannequin: painting the maid's stockings white
    # showed the ragged edge of its shoes' gloss.
    'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS, rigid_post=.075,
                  clip_hem=dict(keep='above', within=.4, per_side=True, shift=-.003),
                  # The sole stood 2.3 cm over the plate boots' sole, whose lift every heel shares.
                  lower_shoe=.023,
                  repaint=dict(mode='clean_knit', rough=.2, margin=4),
                  # The generated red stopped halfway down the heels' inner faces.
                  sole_colour=(.74, .05, .06),
                  # Generated 9 mm ahead of the leg all the way down: moved back as one piece.
                  model=dict(source='tripo_boots_v1/segmented.glb', sha256='0a1cd062e336270aa135f12463aec9eb694d8661b7996299119d7071a96e9b07', scale=1, offset_y=.009)),
    # White puff-sleeved blouse under a pink pinafore bodice, generated alone on a bust. Its cuff frill is white
    # like the sleeve, so the arm is cut by the frill's place at the sleeve's end, not its colour.
    'top': dict(nodes=['top'], version='fitted_v1', hide_underwear=(1.15, 1.7),
                clip_hem=dict(keep='below', within=.25), clip_sleeve=dict(frill=-.012, ruffle=.035),
                align_sleeves='rigid', sleeve_pivot='shoulder', sleeve_end=dict(start=.03, radius=.12, fade=.03),
                # The corset came wider than the body's waist.
                taper_waist=dict(top=1.29, within=.2, clearance=.003),
                model=dict(source='tripo_top_v1/segmented.glb',
                           sha256='326c93118da8241086ee52e2d7f8fb5b2594f4f85460623d94b83610eb2b2727', scale=1)),
    # Pink bell skirt over a lace petticoat with the apron and back bow, generated alone on the bare mannequin.
    # Its pink panels came as pieces of 260 to 1250 faces, so a skirt layer is counted from 250.
    'bottom': dict(MAID['parts']['bottom'],
                   drop_lining=dict(MAID['parts']['bottom']['drop_lining'], layer=250),
                   model=dict(source='tripo_skirt_v1/segmented.glb',
                              sha256='bfc2f9e585606d62ccdd9d46372ad76b41be9d199d029f00a7fbcc1c7d123f38', scale=1)),
})
COVER_RAY = .06
# Sleeve alignment fades in over this distance from the shoulder joint, leaving the shoulder pads in place.
SLEEVE_BLEND = (.03, .09)
# A long glove's opening is measured this far below its top, past the frill and bow (`align_ends`).
GLOVE_TRIM = .04
# The sleeve is cut this far inside the glove cuff, toward the hand.
SLEEVE_CUT_INSET = .01
# A sleeve with `cut_skin_band` is cut this far above its skin-coloured end band.
SKIN_BAND_MARGIN = .003
# A top with `hem_band` (Blender z range searched) is cut along its painted belt band this far below the band's top.
HEM_CUT_DEPTH = .01
# Skin stays visible this far back inside the sleeve so its opening shows no hidden-skin edge; 1 cm showed above the glove cuff.
SLEEVE_SKIN_OVERLAP = .006
# Gloves hide forearm and hand skin this far past their cuff rim and this close to the glove.
CUFF_SKIN_INSET = .01
CUFF_SKIN_GAP = .008
# Shared with the caveman, rogue, barbarian and knight boots, so the baked animation variant is reused.
HEEL_DEGREES = 30
# The generated boots stand on a flat-height ankle. The shoe below the band moves down to the posed sole
# as one piece and the shin tube inside the band stretches; stretching the shoe itself sheared the arch.
ANKLE_BAND = (.18, .32)
# `extend_heel` stretches this much of the stiletto above its tip, from the back of the boot, down to the sole plane.
HEEL_BACK = .18
HEEL_POST = .06
# `upright` (stiletto height above its tip) turns the post vertical about its top; vertices this close to its axis belong to it.
HEEL_POST_RADIUS = .012
# No part is pushed out of the skin (user decision 2026-11-04): skin up to this deep outside a garment is hidden instead.
POKE_DEPTH = .04
SNUG_COVER = .003
# `clip_edge` cuts skin triangles within EDGE_REACH of the edge, leaving skin EDGE_OVERLAP inside it.
EDGE_REACH = .025
EDGE_OVERLAP = .002
EDGE_CLEARANCE = .004
# Skin up to EDGE_LIP in front of the cloth at the rim is tucked EDGE_TUCK behind it.
EDGE_LIP = .008
EDGE_TUCK = .002
# `clip_hem` fits its plane to the open edges within this distance (m) of the cloth's extreme height.
HEM_BAND = .04
# `rigid_collar` takes the weights of the cloth within this height (m) of the collar's base, inside this half width.
COLLAR_RING = .012
COLLAR_HALF_WIDTH = .1
# Depth (m) below the extreme height taken as the cap of an edge generated closed.
HEM_CAP = .01
# Garment points farther than this (m) from the centre plane belong to a sleeve (`clip_sleeve`).
SLEEVE_SIDE = .25
# Open-edge loops shorter than this (m) are slits, not openings.
EDGE_LOOP = .5

argv = sys.argv[sys.argv.index('--') + 1:]
OUTFIT = argv[argv.index('--outfit') + 1]
PART = argv[argv.index('--part') + 1]
PARTS = OUTFITS[OUTFIT]['parts']
SPEC = PARTS[PART]
# A part generated from its own Tripo model overrides the outfit's source and placement with `model`.
CONFIG = {**OUTFITS[OUTFIT], **SPEC.get('model', {})}
RIGID_SLEEVES = SPEC.get('align_sleeves') == 'rigid'
SCALE = CONFIG['scale']
DIRECTORY = ROOT / f'assets/items/female_{OUTFIT}_outfit'
SOURCE = DIRECTORY / CONFIG['source']
SOURCE_SHA256 = CONFIG['sha256']
PREVIEW_FIT = ROOT / f'doc/assets/female-{OUTFIT}-preview-fit.json'
PART_ID = f'{PART}_female_{OUTFIT}'
OUTPUT = DIRECTORY / PART / SPEC['version']
OUTPUT.mkdir(parents=True, exist_ok=True)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


assert digest(SOURCE) == SOURCE_SHA256

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
# The preview skins and masks glTF primitive 0 (the skin), so fit against it rather than Blender's merged mesh.
base, base_raw = read_glb(BODY)
body_primitive = base['meshes'][0]['primitives'][0]
GLTF_TO_BLENDER = np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0]], dtype=float)
to_blender = np.eye(4)
to_blender[:3, :3] = GLTF_TO_BLENDER
rest_points = accessor(base, base_raw, body_primitive['attributes']['POSITION']) @ GLTF_TO_BLENDER.T
rest_normals = accessor(base, base_raw, body_primitive['attributes']['NORMAL']) @ GLTF_TO_BLENDER.T
bf = accessor(base, base_raw, body_primitive['indices']).reshape(-1, 3)
joint_names = [base['nodes'][j]['name'] for j in base['skins'][0]['joints']]
names = [b.name for b in rig.data.bones]
joint_columns = np.array([names.index(n) for n in joint_names])
bw = np.zeros((len(rest_points), len(names)))
np.add.at(bw, (np.arange(len(rest_points))[:, None], joint_columns[accessor(base, base_raw, body_primitive['attributes']['JOINTS_0']).astype(int)]),
          accessor(base, base_raw, body_primitive['attributes']['WEIGHTS_0']))
heel = None
joint_matrices = np.tile(np.eye(4), (len(names), 1, 1))
heel_matrices = joint_matrices.copy()
if PART == 'boots' or CONFIG['on_heels']:
    # heel_pose needs scipy, which Blender's Python lacks.
    script = ('import json, sys; sys.path.insert(0, "tools"); from pathlib import Path; from lib.glb import read_glb; '
              'from outfits.female_boot_pose import heel_pose; '
              f'm, c = heel_pose(*read_glb(Path("{BODY}")), degrees={HEEL_DEGREES}); '
              'print(json.dumps({"deformation": m.tolist(), "config": c}))')
    result = json.loads(subprocess.run([str(ROOT / '.venv/bin/python'), '-c', script], cwd=ROOT,
                                       check=True, capture_output=True, text=True).stdout)
    heel_matrices[joint_columns] = to_blender @ np.array(result['deformation']) @ to_blender.T
    if PART == 'boots':
        joint_matrices, heel = heel_matrices, result['config']
bp = deform_dense(rest_points, bw, joint_matrices)
heel_points = bp if PART == 'boots' else deform_dense(rest_points, bw, heel_matrices) if CONFIG['on_heels'] else rest_points
# Negative: how far the heel pose drops the sole below the floor.
foot_shift = float(heel_points[rest_points[:, 2] < .144, 2].min())
bones = {b.name: np.array(rig.matrix_world @ b.head_local) for b in rig.data.bones}
surface = BVHTree.FromPolygons([Vector(p) for p in bp], bf.tolist())

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = {CONFIG.get('rename', {}).get(o.name, o.name): o for o in set(bpy.data.objects) - before if o.type == 'MESH'}
placement = (Matrix.Translation((0, CONFIG.get('offset_y', 0), (foot_shift if CONFIG['on_heels'] else 0) + CONFIG.get('offset_z', 0)))
             @ Matrix.Scale(SCALE, 4))
world = {n: np.array([placement @ o.matrix_world @ v.co for v in o.data.vertices]) for n, o in imported.items()}


class StillArms:
    """For a model without a named top and gloves: nothing follows the arms."""

    def __call__(self, points, arms, rigid_hand=False):
        return points

    def record(self):
        return None


if {'top', 'left_glove', 'right_glove'} <= world.keys():
    top = world['top']
    pose = ArmPose(bones, bp, bf, bw, names,
                   {'Left': np.vstack([top[top[:, 0] > .2], world['left_glove']]),
                    'Right': np.vstack([top[top[:, 0] < -.2], world['right_glove']])},
                   {'Left': world['left_glove'], 'Right': world['right_glove']},
                   arm_reach=CONFIG.get('arm_reach'), fit_arm=CONFIG.get('fit_arm', False))
else:
    pose = StillArms()
for name, obj in list(imported.items()):
    if name not in SPEC['nodes']:
        bpy.data.objects.remove(obj)
parts = [imported[n] for n in SPEC['nodes']]

bpy.ops.object.select_all(action='DESELECT')
for obj in parts:
    obj.data.transform(placement @ obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
if len(parts) > 1:
    bpy.ops.object.join()
mesh_obj = parts[0]
mesh_obj.name = mesh_obj.data.name = PART_ID
mesh = mesh_obj.data


def base_colour_pixels(material):
    node = next(n for n in material.node_tree.nodes if n.type == 'TEX_IMAGE' and any(
        link.to_socket.name == 'Base Color' for link in n.outputs['Color'].links))
    image = node.image
    pixels = np.empty(len(image.pixels), dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return pixels.reshape(image.size[1], image.size[0], -1)[:, :, :3]


def texel(image, u, v):
    return image[int(np.clip(v, 0, .999) * image.shape[0]), int(np.clip(u % 1, 0, .999) * image.shape[1])]


def component_sizes(positions, faces):
    """Faces in each face's welded component; Tripo splits vertices along UV seams."""
    islands = welded_islands(positions, faces)[2][faces[:, 0]]
    _, inverse, sizes = np.unique(islands, return_inverse=True, return_counts=True)
    return sizes[inverse.ravel()]


def skin_coloured(r, g, b):
    return r > .5 and r > g * 1.15 and g > b * 1.05


def drop_skin():
    """Delete the skin-coloured fingers out of a fingerless glove, then the loose fragments left around them.

    Only connected skin regions go: single faces on leather highlights read as skin and left holes.
    """
    pixels = [base_colour_pixels(m) for m in mesh.materials]
    uv = mesh.uv_layers.active.data
    edit = bmesh.new()
    edit.from_mesh(mesh)
    edit.faces.ensure_lookup_table()
    samples = np.array([(4, 1, 1), (1, 4, 1), (1, 1, 4), (1, 1, 1)], dtype=float) / 6
    is_skin = np.zeros(len(edit.faces), bool)
    for face in edit.faces:
        image = pixels[face.material_index]
        uvs = np.array([uv[loop.index].uv for loop in face.loops[:3]])
        is_skin[face.index] = np.mean([skin_coloured(*texel(image, u, v)) for u, v in samples @ uvs]) >= .5
    positions = np.array([v.co for v in edit.verts])
    corners = np.array([[v.index for v in f.verts] for f in edit.faces])
    skin_faces = np.flatnonzero(is_skin)
    drop = set(skin_faces[component_sizes(positions, corners[skin_faces]) >= 15].tolist())
    kept = np.array([f for f in range(len(corners)) if f not in drop])
    drop |= set(kept[component_sizes(positions, corners[kept]) < .02 * len(kept)].tolist())
    bmesh.ops.delete(edit, geom=[edit.faces[f] for f in drop], context='FACES')
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    return len(drop)


def drop_lining(spec):
    """Delete the shell generated inside a skirt and the scraps it leaves.

    Its faces lie within `near` of the body below `below` with a skirt layer of `layer` faces or more outside
    them; the skirt's own faces over the buttocks lie as near with nothing outside. A bow tail is no layer.
    """
    edit = bmesh.new()
    edit.from_mesh(mesh)
    edit.faces.ensure_lookup_table()
    body = BVHTree.FromPolygons([Vector(p) for p in heel_points], bf.tolist())
    positions = np.array([v.co for v in edit.verts])
    corners = np.array([[v.index for v in f.verts] for f in edit.faces])
    layers = corners[component_sizes(positions, corners) >= spec['layer']]
    own = BVHTree.FromPolygons([Vector(p) for p in positions], layers.tolist())
    drop = []
    for face in edit.faces:
        centre = face.calc_center_median()
        if centre.z > spec['below'] or body.find_nearest(centre)[3] > spec['near']:
            continue
        out = Vector((centre.x, centre.y + .01, 0)).normalized()
        rays = [Matrix.Rotation(np.radians(turn), 3, 'Z') @ out + Vector((0, 0, lift))
                for turn in (-40, -20, 0, 20, 40) for lift in (0, .35)]
        if sum(own.ray_cast(centre + ray.normalized() * .004, ray.normalized(), .4)[0] is not None for ray in rays) >= 8:
            drop.append(face)
    count = len(drop)
    bmesh.ops.delete(edit, geom=drop, context='FACES')
    # Scraps hang by a single corner, so pieces are joined by shared edges. Lacing and bows sit above `scraps`.
    edit.verts.index_update()
    edit.faces.ensure_lookup_table()
    sizes = edge_component_sizes(np.array([v.co for v in edit.verts]),
                                 np.array([[v.index for v in f.verts] for f in edit.faces]))
    scraps = [f for f in edit.faces
              if sizes[f.index] < spec['scrap_faces'] and max(v.co.z for v in f.verts) < spec['scraps']]
    count += len(scraps)
    bmesh.ops.delete(edit, geom=scraps, context='FACES')
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    return count


dropped_skin = drop_skin() if SPEC.get('drop_skin') else 0
dropped_lining = drop_lining(SPEC['drop_lining']) if 'drop_lining' in SPEC else 0
source = np.array([v.co for v in mesh.vertices])
faces = np.array([p.vertices[:] for p in mesh.polygons])


def forearm_hand_columns(side):
    return [i for i, n in enumerate(names) if n == side + 'ForeArm' or n.startswith(side + 'Hand')]


def arm_vertices(points):
    nearest = np.array([bw[bf[surface.find_nearest(Vector(p))[2]]].mean(0) for p in points])
    return {side: nearest[:, [i for i, n in enumerate(names)
                              if n in (side + 'Arm', side + 'ForeArm') or n.startswith(side + 'Hand')]].sum(1) > .5
            for side in ('Left', 'Right')}


points = pose(source, arm_vertices(source), rigid_hand=PART == 'gloves')
if heel and not CONFIG['on_heels']:
    points[:, 2] += foot_shift * (1 - smoothstep((points[:, 2] - ANKLE_BAND[0]) / (ANKLE_BAND[1] - ANKLE_BAND[0])))

def topology(positions, faces):
    _, first, welded = np.unique(np.round(positions, 6), axis=0, return_index=True, return_inverse=True)
    welded = welded.ravel()
    edges = np.unique(np.sort(welded[faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)], axis=1), axis=0)
    adjacency = [[] for _ in first]
    for a, b in edges:
        if a != b:
            adjacency[a].append(int(b))
            adjacency[b].append(int(a))
    return first, welded, adjacency


first, welded, adjacency = topology(source, faces)


def smooth(values, keep, passes):
    for _ in range(passes):
        values = np.array([keep * values[i] + (1 - keep) * values[n].mean(0) if n else values[i]
                           for i, n in enumerate(adjacency)])
    return values


def signed(point):
    hit, normal, _, _ = surface.find_nearest(Vector(point))
    return float(np.dot(point - np.array(hit), np.array(normal)))


depths = np.array([signed(p) for p in points])


def skin_weights(points):
    weights = np.zeros((len(points), len(names)))
    for i, point in enumerate(points):
        hit, _, face, _ = surface.find_nearest(Vector(point))
        a, b, c = bp[bf[face]]
        uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
        bary = np.clip([1 - uv.sum(), *uv], 0, 1)
        weights[i] = (bary / bary.sum()) @ bw[bf[face]]
    weights = smooth(weights[first], .5, 6)[welded]
    if 'rigid_collar' in SPEC:
        # A stand collar and its frill move as one piece with the cloth they stand on. Weighted from the nearest
        # skin, the frill followed the head and lifted off the collar when the head tilted.
        base = SPEC['rigid_collar']
        neck = np.abs(points[:, 0]) < COLLAR_HALF_WIDTH
        ring = weights[neck & (np.abs(points[:, 2] - base) < COLLAR_RING)].mean(0)
        blend = smoothstep((points[:, 2] - base) / COLLAR_RING) * neck
        weights = weights * (1 - blend[:, None]) + ring * blend[:, None]
    for side, sign in (('Left', -1), ('Right', 1)):
        opposite = [j for j, n in enumerate(names) if n.startswith(side)]
        weights[np.ix_(points[:, 0] * sign > .02, opposite)] = 0
    order = np.argsort(weights, axis=1)[:, -4:][:, ::-1]
    values = np.take_along_axis(weights, order, axis=1)
    values[values < .01] = 0
    values /= values.sum(1, keepdims=True)
    order[values == 0] = 0
    return order, values


def chain_param(q, joints):
    """Distance along a chain of joints, the bone direction and the nearest point on it for each point."""
    best = np.full(len(q), np.inf)
    s, axis, foot = np.zeros(len(q)), np.zeros((len(q), 3)), np.zeros((len(q), 3))
    start = 0.
    for a, b in zip(joints, joints[1:]):
        length = np.linalg.norm(b - a)
        direction = (b - a) / length
        t = np.clip((q - a) @ direction, 0, length)
        nearest = a + t[:, None] * direction
        distance = np.linalg.norm(q - nearest, axis=1)
        closer = distance < best
        best[closer], s[closer], axis[closer], foot[closer] = distance[closer], start + t[closer], direction, nearest[closer]
        start += length
    return s, axis, foot


def arm_param(q, side):
    return chain_param(q, [bones[side + n] for n in ('Arm', 'ForeArm', 'Hand')])[:2]


def cross_frame(a, b):
    """Unit axis from a to b and two unit directions across it."""
    axis = (b - a) / np.linalg.norm(b - a)
    across = np.cross(axis, [0, 1, 0])
    across /= np.linalg.norm(across)
    return axis, np.array([across, np.cross(across, axis)])


def arm_frame(side, station, upper):
    start, end = ('Arm', 'ForeArm') if station < upper else ('ForeArm', 'Hand')
    return bones[side + start], *cross_frame(bones[side + start], bones[side + end])


def fit_plane(points):
    centre = points.mean(0)
    return centre, np.linalg.svd(points - centre)[2][2]


def angle_deg(rotation):
    return float(np.degrees(np.arccos(np.clip((np.trace(rotation) - 1) / 2, -1, 1))))


def align_sleeves_rigid(points):
    """Rotate and shift each sleeve as one tube so its end sits on the base arm.

    The sleeve turns about its slice at the shoulder blend so the slice centre at its end lands on the
    arm skin's; everything between follows linearly (user request 2026-11-05). The shoulder blend keeps
    the shoulder seam in place, and that slice moves front/back only, as in `align_sleeves`.
    """
    sleeve_shift = {}
    arms = arm_vertices(points)
    for side in ('Left', 'Right'):
        sleeve = arms[side]
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm')]
        skin = rest_points[bw[:, columns].sum(1) > .5]
        s_sleeve, _ = arm_param(points[sleeve], side)
        s_skin, _ = arm_param(skin, side)
        upper = np.linalg.norm(bones[side + 'ForeArm'] - bones[side + 'Arm'])
        stations = [SLEEVE_BLEND[1], s_sleeve.max() - .015]

        def centres(q, s):
            out = []
            for station in stations:
                _, axis, frame = arm_frame(side, station, upper)
                near = q[np.abs(s - station) < .01]
                coords = near @ frame.T
                out.append((coords.min(0) + coords.max(0)) / 2 @ frame + (near @ axis).mean() * axis)
            return np.array(out)

        sleeve_c, skin_c = centres(points[sleeve], s_sleeve), centres(skin, s_skin)
        pivot = sleeve_c[0]
        offset = np.array([0, (skin_c[0] - pivot)[1], 0])
        # A sleeve ending near the blend leaves too short a baseline between its slices.
        if SPEC.get('sleeve_pivot') == 'shoulder':
            pivot, offset = bones[side + 'Arm'], np.zeros(3)
        rotation = rotation_between(sleeve_c[1] - pivot, skin_c[1] - pivot - offset)
        q = points[sleeve]
        moved = (q - pivot) @ rotation.T + pivot + offset
        weight = smoothstep((s_sleeve - SLEEVE_BLEND[0]) / (SLEEVE_BLEND[1] - SLEEVE_BLEND[0]))[:, None]
        shift = weight * (moved - q)
        points[sleeve] = q + shift
        sleeve_shift[side] = {'stations_m': [float(v) for v in stations], 'rotation_deg': angle_deg(rotation),
                              'shoulder_offset_mm_y': float(offset[1] * 1000), 'vertices': int(sleeve.sum()),
                              'max_mm': float(np.linalg.norm(shift, axis=1).max() * 1000),
                              'end_residual_mm': float(np.linalg.norm(centres(points[sleeve], s_sleeve)[-1] - skin_c[-1]) * 1000)}
    return sleeve_shift


def align_sleeves(points):
    sleeve_shift = {}
    # Shift each sleeve so its centre line follows the base arm's; shape and thickness stay.
    # The shoulder station keeps only front/back (Blender y): a sideways shift there opened gaps under the shoulder pads.
    # Elbow and wrist correct every direction across the arm.
    arms = arm_vertices(points)
    for side in ('Left', 'Right'):
        sleeve = arms[side]
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm')]
        skin = rest_points[bw[:, columns].sum(1) > .5]
        s_sleeve, axis_sleeve = arm_param(points[sleeve], side)
        s_skin, _ = arm_param(skin, side)
        # Matching every 2 cm made the sleeve wavy, so only shoulder, elbow and wrist are matched, linear between.
        upper = np.linalg.norm(bones[side + 'ForeArm'] - bones[side + 'Arm'])
        lower = np.linalg.norm(bones[side + 'Hand'] - bones[side + 'ForeArm'])
        stations = [SLEEVE_BLEND[1], upper, upper + lower - .02]
        offsets = []
        for index, station in enumerate(stations):
            near_sleeve = np.abs(s_sleeve - station) < .03
            near_skin = np.abs(s_skin - station) < .03
            assert near_sleeve.sum() > 8 and near_skin.sum() > 8, f'{side} sleeve station {station:.3f} has too few points'
            start, end = [('Arm', 'ForeArm'), ('Arm', 'Hand'), ('ForeArm', 'Hand')][index]
            _, frame = cross_frame(bones[side + start], bones[side + end])

            def middle(q):
                # Middle of the slice's extent across the arm; a vertex mean leaned toward densely meshed spots.
                coords = q @ frame.T
                return (coords.min(0) + coords.max(0)) / 2 @ frame

            offset = middle(skin[near_skin]) - middle(points[sleeve][near_sleeve])
            if index == 0:
                offset = np.array([0, offset[1], 0])
            offsets.append(offset)
        offsets = np.array(offsets)
        shift = np.column_stack([np.interp(s_sleeve, stations, offsets[:, k]) for k in range(3)])
        shift -= (shift * axis_sleeve).sum(1, keepdims=True) * axis_sleeve
        shift *= smoothstep((s_sleeve - SLEEVE_BLEND[0]) / (SLEEVE_BLEND[1] - SLEEVE_BLEND[0]))[:, None]
        points[sleeve] += shift
        sleeve_shift[side] = {'stations_m': [float(v) for v in stations], 'offsets_mm_xyz': (offsets * 1000).round(1).tolist(),
                              'vertices': int(sleeve.sum()), 'max_mm': float(np.linalg.norm(shift, axis=1).max() * 1000),
                              'median_mm': float(np.median(np.linalg.norm(shift, axis=1)) * 1000)}
    return sleeve_shift


def align_glove_ends(points):
    """Turn and move each long glove as one piece so its wrist and its opening are centred on the base arm."""
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        glove = points[:, 0] * sign > 0
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm')]
        skin = rest_points[bw[:, columns].sum(1) > .5]
        s_glove, _ = arm_param(points[glove], side)
        s_skin, _ = arm_param(skin, side)
        upper = np.linalg.norm(bones[side + 'ForeArm'] - bones[side + 'Arm'])
        lower = np.linalg.norm(bones[side + 'Hand'] - bones[side + 'ForeArm'])
        stations = [upper + lower - .02, s_glove.min() + GLOVE_TRIM]

        def centres(q, s):
            out = []
            for station in stations:
                origin, axis, frame = arm_frame(side, station, upper)
                near = q[np.abs(s - station) < .015]
                assert len(near) > 8, f'{side} glove station {station:.3f} has too few points'
                # Percentiles: a bow or seam on one side pulled the plain extent off centre.
                low, high = np.percentile(near @ frame.T, [5, 95], axis=0)
                out.append((low + high) / 2 @ frame + (origin @ axis + station - (0 if station < upper else upper)) * axis)
            return np.array(out)

        glove_c, skin_c = centres(points[glove], s_glove), centres(skin, s_skin)
        rotation = rotation_between(glove_c[1] - glove_c[0], skin_c[1] - skin_c[0])
        moved = (points[glove] - glove_c[0]) @ rotation.T + skin_c[0]
        shift = np.linalg.norm(moved - points[glove], axis=1)
        points[glove] = moved
        after = centres(points[glove], arm_param(points[glove], side)[0])
        record[side] = {'stations_m': [float(v) for v in stations], 'rotation_deg': angle_deg(rotation),
                        'offsets_before_mm': (np.linalg.norm(glove_c - skin_c, axis=1) * 1000).round(1).tolist(),
                        'offsets_after_mm': (np.linalg.norm(after - skin_c, axis=1) * 1000).round(1).tolist(),
                        'max_mm': float(shift.max() * 1000)}
    return record


def closed_rim(own, up):
    """Edge points of a tube generated closed, on one leg: the highest point in each direction, less spikes.

    A band below the highest point caught only the high side of a tilted top.
    """
    height = own[:, 2].max() if up == 1 else own[:, 2].min()
    top = own[(own[:, 2] - height) * up > -.08]
    angle = np.arctan2(*(top[:, :2] - top[:, :2].mean(0)).T)
    sector = np.floor((angle + np.pi) / (2 * np.pi) * 48).astype(int) % 48
    rim = np.array([top[sector == k][np.argmax(top[sector == k][:, 2] * up)] for k in np.unique(sector)])
    for _ in range(3):
        design = np.c_[rim[:, :2], np.ones(len(rim))]
        rim = rim[np.abs(rim[:, 2] - design @ np.linalg.lstsq(design, rim[:, 2], rcond=None)[0]) < .004]
    return rim


def level_tops(points):
    """Press whatever stands above each stocking's edge plane down onto it: spikes showed over the lace."""
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        leg = points[:, 0] * sign > 0
        centre, normal = fit_plane(closed_rim(points[leg], 1))
        normal *= np.sign(normal[2])
        above = np.maximum((points[leg] - centre) @ normal, 0)
        points[leg] -= above[:, None] * normal
        record[side] = {'vertices': int((above > 1e-5).sum()), 'max_mm': float(above.max() * 1000)}
    return record


def plane_section(vertices, triangles, origin, axis):
    """Points where a surface crosses a plane: vertex slabs missed rings on the sparse body."""
    depth = (vertices - origin) @ axis
    cut = []
    for i, k in ((0, 1), (1, 2), (2, 0)):
        da, db = depth[triangles[:, i]], depth[triangles[:, k]]
        crossing = (da * db < 0)
        t = (da[crossing] / (da[crossing] - db[crossing]))[:, None]
        cut.append(vertices[triangles[crossing, i]] * (1 - t) + vertices[triangles[crossing, k]] * t)
    return np.vstack(cut)


def taper_waist(points, faces, spec):
    """Scale a bodice's waist across the spine so its bottom edge is as wide as the base body.

    Measured at the bottom edge only and run linearly up to `top`, under the bust, where nothing moves.
    """
    torso = np.abs(points[:, 0]) < spec['within']
    bottom = points[torso, 2].min() + .01
    up = np.array([0., 0, 1])

    def extent(vertices, triangles):
        cut = plane_section(vertices, triangles, np.array([0, 0, bottom]), up)[:, :2]
        cut = cut[np.abs(cut[:, 0]) < spec['within']]
        return np.ptp(cut, axis=0), (cut.min(0) + cut.max(0)) / 2

    cloth_size, cloth_centre = extent(points, faces)
    skin_size, skin_centre = extent(rest_points, bf)
    factor = (skin_size.sum() + 4 * spec['clearance']) / cloth_size.sum()
    t = np.clip((spec['top'] - points[torso, 2]) / (spec['top'] - bottom), 0, 1)[:, None]
    points[torso, :2] = (cloth_centre + (points[torso, :2] - cloth_centre) * (1 + (factor - 1) * t)
                         + (skin_centre - cloth_centre) * t)
    return {'bottom_m': float(bottom), 'top_m': spec['top'], 'factor': float(factor),
            'cloth_size_m': cloth_size.round(4).tolist(), 'skin_size_m': skin_size.round(4).tolist(),
            'shift_mm': ((skin_centre - cloth_centre) * 1000).round(1).tolist()}


def taper_stockings(points, faces, spec):
    """Scale each stocking across the leg so its opening and its ankle are as wide as the base leg.

    Only the two ends are measured and the factor runs linearly between them, which keeps the tube smooth.
    The opening is also centred on the thigh, a shift that falls to nothing at the ankle. Below the ankle
    the factor fades out, leaving the shoe as generated.
    """
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        joints = [bones[side + n] for n in ('UpLeg', 'Leg', 'Foot')]
        thigh = np.linalg.norm(joints[1] - joints[0])
        ankle = thigh + np.linalg.norm(joints[2] - joints[1]) - spec['ankle']
        leg = points[:, 0] * sign > 0
        s_cloth, _, foot_cloth = chain_param(points[leg], joints)
        tube = np.linalg.norm(points[leg] - foot_cloth, axis=1) < spec['reach']
        stations = [s_cloth[tube].min() + spec['opening'], ankle]
        factors, centres = [], []
        for station in stations:
            a, b, along = (joints[0], joints[1], station) if station < thigh else (joints[1], joints[2], station - thigh)
            axis, frame = cross_frame(a, b)
            origin = a + along * axis

            def section(vertices, triangles):
                cut = plane_section(vertices, triangles, origin, axis)
                cut = cut[(cut[:, 0] * sign > 0) & (np.linalg.norm(cut - origin, axis=1) < spec['reach'])]
                assert len(cut) > 8, f'{side} stocking station {station:.3f} has too few points'
                coords = (cut - origin) @ frame.T
                return np.ptp(coords, axis=0), (coords.min(0) + coords.max(0)) / 2

            cloth_size, cloth_centre = section(points, faces)
            skin_size, skin_centre = section(bp, bf)
            factors.append((skin_size.sum() + 4 * spec['clearance']) / cloth_size.sum())
            centres.append((skin_centre - cloth_centre * factors[-1]) @ frame)
        fade = 1 - smoothstep((s_cloth - ankle) / spec['ankle_fade'])
        factor = 1 + (np.interp(s_cloth, stations, factors) - 1) * fade
        shift = centres[0] * np.interp(s_cloth, stations, [1, 0])[:, None]
        points[leg] = foot_cloth + (points[leg] - foot_cloth) * factor[:, None] + shift
        record[side] = {'stations_m': [float(v) for v in stations], 'factors': [float(v) for v in factors],
                        'opening_shift_mm': (centres[0] * 1000).round(1).tolist(),
                        'ankle_offset_mm': (centres[1] * 1000).round(1).tolist()}
    return record


def repaint(spec):
    """Recolour this part's texels in the base colour texture; other parts share the atlas untouched.

    `sheer`: skin colour blended into the matte knit so the leg seems to show through. The glossy shoe (low
    roughness) keeps its colour, and so does the lace and welt band, `band` metres down from each stocking's
    top edge. Picking the band by its tone left the knit's baked shadows black too.
    `clean_knit`: white knit stained by the shoe's black is made white again.
    `white_cloth`: dark cloth turns white, its highlights and folds kept as shading.
    """
    def image_of(socket):
        shader = next(n for n in mesh.materials[0].node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        node = shader.inputs[socket].links[0].from_node
        while node.type != 'TEX_IMAGE':
            node = next(i.links[0].from_node for i in node.inputs if i.links)
        return node.image

    def read(image):
        values = np.empty(len(image.pixels), dtype=np.float32)
        image.pixels.foreach_get(values)
        return values.reshape(image.size[1], image.size[0], -1)

    colour_image = image_of('Base Color')
    colour, rough = read(colour_image), read(image_of('Roughness'))[:, :, 1]
    assert rough.shape == colour.shape[:2]
    height, width = rough.shape
    position = np.array([v.co for v in mesh.vertices])
    uv = mesh.uv_layers.active.data
    below = np.zeros(len(position))
    if 'band' in spec:
        for sign in (1, -1):
            leg = position[:, 0] * sign > 0
            centre, normal = fit_plane(closed_rim(position[leg], 1))
            below[leg] = (centre - position[leg]) @ (normal * np.sign(normal[2]))
    if spec['mode'] == 'clean_knit':
        below = np.array([v.normal.z for v in mesh.vertices])
    depth = np.full((height, width), -1, np.float32)
    used = np.zeros((height, width), bool)
    # A light colour needs a wider rim than 1.5 px: the game's 1024 px texture mixed the black past the islands'
    # edges into grey seams.
    margin = spec.get('margin', 1.5)
    for polygon in mesh.polygons:
        corners = np.array([uv[i].uv for i in polygon.loop_indices]) * (width, height)
        low = np.maximum(np.floor(corners.min(0) - margin - .5).astype(int), 0)
        high = np.minimum(np.ceil(corners.max(0) + margin + .5).astype(int), (width - 1, height - 1))
        x, y = np.meshgrid(np.arange(low[0], high[0] + 1) + .5, np.arange(low[1], high[1] + 1) + .5)
        a, b, c = corners
        area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        if abs(area) < 1e-9:
            continue
        # Barycentric weights; texels up to `margin` px outside an island's edge are taken too.
        inside = np.ones(x.shape, bool)
        value = np.zeros(x.shape)
        for (p, q), opposite in zip(((b, c), (c, a), (a, b)), polygon.vertices):
            edge = ((q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])) / area
            inside &= edge * abs(area) / np.hypot(*(q - p)) > -margin
            value += edge * below[opposite]
        depth[low[1]:high[1] + 1, low[0]:high[0] + 1][inside] = value[inside]
        used[low[1]:high[1] + 1, low[0]:high[0] + 1] |= inside
    rgb = colour[:, :, :3]
    tone = rgb.mean(2)
    knit = smoothstep((rough - .4) / .2)
    under_band = smoothstep((depth - spec.get('band', 0)) / .004)
    if spec['mode'] == 'sheer':
        weight = spec['amount'] * knit * under_band
        target = np.array(spec['skin'], np.float32)
    elif spec['mode'] == 'clean_knit':
        # The shoe's black ran over its edge onto the matte knit: knit texels (rougher than `rough`) take the
        # knit's own white.
        white = np.median(rgb[used & (tone > .7)], axis=0)
        target = np.maximum(rgb, white)
        # The red sole is as matte as the knit and stays: by its colour, and in shadow by facing down.
        weight = (used * smoothstep((rough - spec['rough']) / .03) * (rgb[:, :, 0] < 1.3 * rgb[:, :, 2] + .02)
                  * (depth > -.3))
    else:
        target = np.maximum(rgb, np.clip(.78 + .9 * tone, 0, .97)[:, :, None] * np.array(spec['white'], np.float32))
        weight = used.astype(np.float32)
    rgb += (target - rgb) * weight[:, :, None]
    colour_image.pixels.foreach_set(colour.ravel())
    colour_image.update()
    colour_image.pack()
    return int((weight > .01).sum())


def cut_hem(edit):
    """Cut the top's lower edge along the top of its painted belt band, which the skirt's belt covers.

    The band is found from the texture, so the cut follows its curve (low at the front, high at the sides).
    Coordinates are shifted so that curve becomes a flat plane for bisect_plane, then shifted back.
    """
    uv = edit.loops.layers.uv.active
    pixels = [base_colour_pixels(m) for m in mesh.materials]
    hem = [f for f in edit.faces if any(SPEC['hem_band'][0] < v.co.z < SPEC['hem_band'][1] for v in f.verts)
           and all(abs(v.co.x) < .2 for v in f.verts)]
    # The band is painted across large triangles, so sample the texture inside each one, not at its corners.
    weights = np.array([(i, j, 8 - i - j) for i in range(9) for j in range(9 - i)], dtype=float) / 8
    belt = []
    for face in hem:
        if len(face.loops) != 3:
            continue
        image = pixels[face.material_index]
        height, width = image.shape[:2]
        corners = np.array([loop.vert.co for loop in face.loops])
        uvs = np.array([loop[uv].uv for loop in face.loops])
        for position, (u, v) in zip(weights @ corners, weights @ uvs):
            r, g, b = texel(image, u, v)
            if r > g * 1.15 and g > b * 1.1 and r < .75:
                belt.append(position)
    belt = np.array(belt)
    centre = belt.mean(0)
    sectors = 18

    def sector_of(co):
        return int((np.arctan2(co[0] - centre[0], co[1] - centre[1]) + np.pi) / (2 * np.pi) * sectors) % sectors

    tops = {}
    for co in belt:
        k = sector_of(co)
        tops.setdefault(k, []).append(co[2])
    known = sorted(tops)
    angles = (np.array(known) + .5) / sectors * 2 * np.pi - np.pi
    # Top of the band per direction; the 95th percentile ignores stray brown texels above it.
    heights = np.array([np.percentile(tops[k], 95) for k in known])

    def cut_height(co):
        angle = np.arctan2(co[0] - centre[0], co[1] - centre[1])
        return float(np.interp(angle, angles, heights, period=2 * np.pi)) - HEM_CUT_DEPTH

    torso = [v for v in edit.verts if SPEC['hem_band'][0] - .1 < v.co.z < SPEC['hem_band'][1] + .05 and abs(v.co.x) < .25]
    shift = {v: cut_height(v.co) for v in torso}
    for v in torso:
        v.co.z -= shift[v]
    torso_faces = [f for f in edit.faces if all(v in shift for v in f.verts)]
    geometry = torso + list({e for f in torso_faces for e in f.edges}) + torso_faces
    result = bmesh.ops.bisect_plane(edit, geom=geometry, plane_co=Vector((0, 0, 0)), plane_no=Vector((0, 0, 1)))
    new_verts = [e for e in result['geom_cut'] if isinstance(e, bmesh.types.BMVert)]
    for v in new_verts:
        shift[v] = cut_height(Vector((v.co.x, v.co.y, 0)))
    below = [v for v in shift if v.is_valid and v.co.z < -1e-6]
    for v in shift:
        if v.is_valid:
            v.co.z += shift[v]
    bmesh.ops.delete(edit, geom=below, context='VERTS')
    record = {'band_sectors': len(known), 'band_top_heights_m': [round(float(h), 4) for h in heights],
              'removed_vertices': len(below), 'cut_vertices': len(new_verts), 'depth_below_band_top_m': HEM_CUT_DEPTH}
    return record, cut_height


def cuff_plane(glove, side):
    """Plane fitted to the glove cuff's top rim, its normal pointing toward the hand.

    The rim is slanted about 3 cm, so a plane square to the forearm would miss it on one side.
    """
    axis = bones[side + 'Hand'] - bones[side + 'ForeArm']
    axis /= np.linalg.norm(axis)
    along = (glove - bones[side + 'ForeArm']) @ axis
    across = glove - bones[side + 'ForeArm'] - along[:, None] * axis
    reference = np.cross(axis, [0, 1, 0])
    angle = np.arctan2(across @ np.cross(axis, reference), across @ reference)
    rim = along < along.min() + .04
    sectors = ((angle[rim] + np.pi) / (2 * np.pi) * 16).astype(int) % 16
    rim_points = np.array([glove[rim][sectors == k][np.argmin(along[rim][sectors == k])] for k in np.unique(sectors)])
    centre = rim_points.mean(0)
    normal = np.linalg.svd(rim_points - centre)[2][2]
    return centre, (normal if normal @ axis > 0 else -normal), axis


def cut_sleeves(points):
    """Cut each sleeve with a plane across the forearm just inside the glove cuff.

    The sleeve's end was ragged and its last band was skin-coloured; the glove hides the clean cut edge.
    """
    gloves_doc, gloves_raw = read_glb(DIRECTORY / 'gloves' / PARTS['gloves']['version'] / f'gloves_female_{OUTFIT}.glb')
    glove = np.vstack([accessor(gloves_doc, gloves_raw, primitive['attributes']['POSITION'])
                       for m in gloves_doc['meshes'] for primitive in m['primitives']]) @ GLTF_TO_BLENDER.T
    for vertex, point in zip(mesh.vertices, points):
        vertex.co = point
    edit = bmesh.new()
    edit.from_mesh(mesh)
    pixels = [base_colour_pixels(m) for m in mesh.materials] if SPEC.get('cut_skin_band') else []
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        centre, normal, axis = cuff_plane(glove[glove[:, 0] * sign > 0], side)
        cuff_top = float((centre - bones[side + 'ForeArm']) @ axis)
        cut = centre + normal * SLEEVE_CUT_INSET
        # Only this sleeve is cut; the plane also crosses the bodice.
        elbow, reach = bones[side + 'ForeArm'], CONFIG.get('arm_reach')

        def sleeve(v):
            # A hauberk's hip lies beyond the plane too, so with `arm_reach` only vertices near the forearm count.
            along = np.array(v.co) - elbow
            return v.co.x * sign > abs(bones[side + 'Arm'][0]) and (
                not reach or np.linalg.norm(along - (along @ axis) * axis) < reach)

        on_side = [v for v in edit.verts if sleeve(v)]
        side_faces = [f for f in edit.faces if all(sleeve(v) for v in f.verts)]
        band = 0.
        if SPEC.get('cut_skin_band'):
            # A sleeve ending short of the cuff, bare once the gloves come off, is cut cleanly above its
            # skin-coloured end band; dropping those faces left a ragged end.
            uv = edit.loops.layers.uv.active
            depths = []
            for face in side_faces:
                image = pixels[face.material_index]
                u, v = np.mean([loop[uv].uv for loop in face.loops], axis=0)
                depth = (np.mean([np.array(c.co) for c in face.verts], axis=0) - cut) @ normal
                if depth > -.08 and skin_coloured(*texel(image, u, v)):
                    depths.append(depth)
            band = min(0., float(np.percentile(depths, 5)) - SKIN_BAND_MARGIN)
            cut = cut + normal * band
        geometry = on_side + list({e for f in side_faces for e in f.edges}) + side_faces
        result = bmesh.ops.bisect_plane(edit, geom=geometry, plane_co=Vector(cut), plane_no=Vector(normal))
        beyond = [v for v in on_side if v.is_valid and (np.array(v.co) - cut) @ normal > 1e-6]
        bmesh.ops.delete(edit, geom=beyond, context='VERTS')
        record[side] = {'plane': (cut.tolist(), normal.tolist()), 'cuff_top_from_elbow_m': cuff_top,
                        **({'skin_band_cut_inside_m': -band} if band else {}),
                        'rim_tilt_deg': float(np.degrees(np.arccos(np.clip(normal @ axis, -1, 1)))),
                        'removed_vertices': len(beyond), 'cut_vertices': sum(isinstance(e, bmesh.types.BMVert) for e in result['geom_cut'])}
    bmesh.ops.triangulate(edit, faces=edit.faces[:])
    hem = hem_height = None
    if 'hem_band' in SPEC:
        hem, hem_height = cut_hem(edit)
        bmesh.ops.triangulate(edit, faces=edit.faces[:])
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    cut_points = np.array([v.co for v in mesh.vertices])
    cut_faces = np.array([poly.vertices[:] for poly in mesh.polygons])
    return cut_points, cut_faces, record, hem, hem_height




def extend_heels(posed, extra):
    """Turn each stiletto upright about its top, then stretch it toward its boot's lowest sole point, in the heel pose
    (standing feet are level there)."""
    posed = posed.copy()
    record = {}
    for side, sign in (('left', 1), ('right', -1)):
        foot = np.flatnonzero(np.sign(posed[:, 0]) == sign)
        y = posed[foot, 1]
        back = foot[y > y.max() - HEEL_BACK * (y.max() - y.min())]
        record[side] = {}
        if 'upright' in extra:
            height = extra['upright']
            tip = posed[back, 2].min()
            rise = posed[back, 2] - tip
            base = posed[back[rise < .004]].mean(0)
            pivot = posed[back[np.abs(rise - height) < .004]].mean(0)
            axis = base + np.clip(rise / height, 0, 1)[:, None] * (pivot - base)
            post = back[(rise < height + .01) & (np.linalg.norm(posed[back, :2] - axis[:, :2], axis=1) < HEEL_POST_RADIUS)]
            # Rigid turn about the top, fading out over the last centimetre so the heel cup is untouched.
            # `lean` sets the tip that far toward the toe (-y) of plumb.
            down = base - pivot
            target = np.array([0, -extra.get('lean', 0), down[2]])
            turn = np.cross(down, target)
            angle = np.arctan2(np.linalg.norm(turn), down @ target)
            turn /= np.linalg.norm(turn)
            weight = np.clip((pivot[2] - posed[post, 2]) / .01, 0, 1)
            for vertex, amount in zip(post, weight):
                a = angle * amount
                offset = posed[vertex] - pivot
                posed[vertex] = pivot + offset * np.cos(a) + np.cross(turn, offset) * np.sin(a) + turn * (turn @ offset) * (1 - np.cos(a))
            record[side].update(upright_degrees=float(np.degrees(angle)), upright_vertices=len(post))
        sole, tip = posed[foot, 2].min(), posed[back, 2].min()
        # `gap` stretches both heels by the same length instead of each to its own sole.
        gap = extra['gap'] if 'gap' in extra else (tip - sole + extra[side]) * extra.get('share', 1)
        if gap > 0:
            weight = np.clip(1 - (posed[back, 2] - tip) / HEEL_POST, 0, 1)
            posed[back, 2] -= weight * gap
            record[side].update(gap_m=float(gap), stretched_vertices=int((weight > 0).sum()))
    return posed, record


def rigid_posts(posed, order, values, height):
    """Give each stiletto, up to `height` above its tip, the weights of its top ring so it moves as one piece.
    The tip's nearest skin is the ankle, whose shin weight bent the tip away from the post."""
    dense = np.zeros((len(posed), len(names)))
    np.add.at(dense, (np.arange(len(posed))[:, None], order), values)
    count = 0
    for sign in (1, -1):
        foot = np.flatnonzero(np.sign(posed[:, 0]) == sign)
        y = posed[foot, 1]
        back = foot[y > y.max() - HEEL_BACK * (y.max() - y.min())]
        rise = posed[back, 2] - posed[back, 2].min()
        tip = posed[back[rise < .004]].mean(0)
        near = np.linalg.norm(posed[back, :2] - tip[:2], axis=1) < .03
        post = back[near & (rise < height)]
        ring = back[near & (rise > height - .015) & (rise < height)]
        if not len(ring):
            # A stiletto of long triangles has no vertex in that band: the next ring up stands in.
            above = near & (rise >= height)
            ring = back[above & (rise < rise[above].min() + .015)]
        dense[post] = dense[ring].mean(0)
        count += len(post)
    order = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    values = np.take_along_axis(dense, order, axis=1)
    values[values < .01] = 0
    values /= values.sum(1, keepdims=True)
    order[values == 0] = 0
    return order, values, count


def paint_sole(posed, bind, colour):
    """Give the sole, arch and heel tip (facing down) and the stiletto's toe-facing side a coloured copy of the leather,
    in the heel pose, like the rogue boots' red sole. The heel's sides and back keep the leather."""
    tri = posed[faces]
    normals = np.cross(tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0])
    normals /= np.maximum(np.linalg.norm(normals, axis=1, keepdims=True), 1e-12)
    centres = tri.mean(1)
    painted = np.zeros(len(faces), bool)
    for sign in (1, -1):
        side = np.sign(centres[:, 0]) == sign
        sole = posed[faces[side]].reshape(-1, 3)[:, 2].min()
        y = centres[side, 1]
        back = y > y.max() - HEEL_BACK * (y.max() - y.min())
        rise = centres[side, 2] - sole
        # Down-facing faces tilted sideways are the arch's edge, which shows from the side.
        down = (normals[side, 2] < -.5) & (np.abs(normals[side, 0]) < .4) & (rise < .11)
        inner = back & (rise < .1) & (normals[side, 1] < -.3)
        painted[np.flatnonzero(side)[down | inner]] = True
    # Triangles clicked in the preview, matched by their bind-space centre in glTF axes.
    saved = json.loads(PREVIEW_FIT.read_text())['parts'].get(PART, {}).get('sole', {}) if PREVIEW_FIT.exists() else {}
    if saved.get('colour'):
        colour = tuple(int(saved['colour'][i:i + 2], 16) / 255 for i in (1, 3, 5))
    centres = bind[faces].mean(1) @ TO_GLTF.T
    for value, key in ((True, 'gold'), (False, 'leather')):
        for point in saved.get(key, []):
            distance = np.linalg.norm(centres - point, axis=1)
            if distance.min() < .004:
                painted[distance.argmin()] = value
    material = mesh.materials[0].copy()
    material.name = f'{PART_ID}_sole'
    shader = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    # The leather's roughness and normal maps left glossy pale patches on the flat colour.
    for name in ('Base Color', 'Roughness', 'Metallic', 'Normal'):
        for link in list(shader.inputs[name].links):
            material.node_tree.links.remove(link)
    linear = [c / 12.92 if c <= .04045 else ((c + .055) / 1.055) ** 2.4 for c in colour]
    shader.inputs['Base Color'].default_value = (*linear, 1)
    shader.inputs['Roughness'].default_value = .6
    shader.inputs['Metallic'].default_value = 0
    mesh.materials.append(material)
    for polygon in mesh.polygons:
        if painted[polygon.index]:
            polygon.material_index = len(mesh.materials) - 1
    return int(painted.sum()), colour


paired = PART in ('gloves', 'boots')
keep_pending = '--keep-pending' in argv
sole_faces = 0
sole_colour = None
# Glove fits saved in the forearm frame need the elbow and wrist.
arms = {sign: (bones[side + 'ForeArm'] @ TO_GLTF.T, bones[side + 'Hand'] @ TO_GLTF.T)
        for side, sign in (('Left', 1), ('Right', -1))}
heel_extension = {}
levelled_tops = {}
waist_taper = {}
stocking_taper = {}
glove_align = {}
sleeve_shift = {}
sleeve_cut = {}
hem_cut = hem_height = None
if heel:
    # Boots are skinned in the heel pose, then returned to bind space where preview adjustments apply.
    if 'taper' in SPEC:
        stocking_taper = taper_stockings(points, faces, SPEC['taper'])
    if SPEC.get('level_tops'):
        levelled_tops = level_tops(points)
    order, values = skin_weights(points)
    if 'rigid_post' in SPEC:
        order, values, rigid_post_vertices = rigid_posts(points, order, values, SPEC['rigid_post'])
    skinning = np.einsum('nk,nkij->nij', values, joint_matrices[order])
    if 'lower_shoe' in SPEC:
        # As with a flat-posed boot, the shoe below the ankle band moves as one piece and the tube inside the band stretches.
        band = np.array(ANKLE_BAND) - foot_shift
        points[:, 2] -= SPEC['lower_shoe'] * (1 - smoothstep((points[:, 2] - band[0]) / (band[1] - band[0])))
    points = np.linalg.solve(skinning, np.c_[points, np.ones(len(points))][..., None])[:, :3, 0]
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending, arms=arms)
    if SPEC.get('extend_heel'):
        homogeneous = np.c_[points, np.ones(len(points))][..., None]
        posed, heel_extension = extend_heels((skinning @ homogeneous)[:, :3, 0], SPEC['extend_heel'])
        points = np.linalg.solve(skinning, np.c_[posed, np.ones(len(posed))][..., None])[:, :3, 0]
    if 'sole_colour' in SPEC:
        homogeneous = np.c_[points, np.ones(len(points))][..., None]
        sole_faces, sole_colour = paint_sole((skinning @ homogeneous)[:, :3, 0], points, SPEC['sole_colour'])
else:
    # Weights come last so they are sampled where the adjusted, aligned vertices end up.
    # One-tube sleeves are aligned before the saved fits, which then move them as the preview showed.
    if RIGID_SLEEVES:
        sleeve_shift = align_sleeves_rigid(points)
    if 'taper_waist' in SPEC:
        waist_taper = taper_waist(points, faces, SPEC['taper_waist'])
    if SPEC.get('align_ends'):
        glove_align = align_glove_ends(points)
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending, arms=arms)
    if SPEC.get('align_sleeves') is True:
        sleeve_shift = align_sleeves(points)
    if SPEC.get('cut_inside_glove'):
        points, faces, sleeve_cut, hem_cut, hem_height = cut_sleeves(points)
        first, welded, adjacency = topology(points, faces)
    if RIGID_SLEEVES:
        sleeve_ends = sleeve_end_total(preview_fits)
        joints = {sign: (bones[side + 'Arm'] @ TO_GLTF.T, bones[side + 'ForeArm'] @ TO_GLTF.T)
                  for side, sign in (('Left', 1), ('Right', -1))}
        points = sleeve_end(points, joints, sleeve_ends, **SPEC.get('sleeve_end', {}))
    order, values = skin_weights(points)

for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
face_normals = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
normal_sum = np.zeros((len(first), 3))
for k in range(3):
    np.add.at(normal_sum, welded[faces[:, k]], face_normals)
normal_sum /= np.maximum(np.linalg.norm(normal_sum, axis=1, keepdims=True), 1e-12)
mesh.normals_split_custom_set([normal_sum[welded[loop.vertex_index]] for loop in mesh.loops])
for key, data in (('_RIG_JOINTS', order), ('_RIG_WEIGHTS', values)):
    attribute = mesh.attributes.new(key, 'FLOAT_COLOR', 'POINT')
    attribute.data.foreach_set('color', data.astype(np.float32).ravel())

garment = BVHTree.FromPolygons([Vector(p) for p in points], faces.tolist())


def snug(p):
    return CONFIG.get('snug_cover') and garment.find_nearest(Vector(p), SNUG_COVER)[0] is not None


def covers(p, n):
    return (garment.ray_cast(Vector(p + n * .001), Vector(n), COVER_RAY)[0] is not None
            or garment.ray_cast(Vector(p), Vector(-n), POKE_DEPTH)[0] is not None
            # Skin lying on the garment surface slips between both rays and z-fights through it.
            or snug(p))


golden = np.pi * (3 - np.sqrt(5))
OPENING_CONE = .3
SNUG = .006
MOTION_CLIPS = ('idle1', 'walk', 'run', 'combat_idle', 'slash1')
MOTION_FRAMES = 6
VIEW_RAYS = np.array([[np.cos(golden * i) * np.sqrt(1 - z * z), np.sin(golden * i) * np.sqrt(1 - z * z), z]
                      for i, z in enumerate(np.linspace(-.95, .95, 48))])


def seen_through_opening(p, n):
    """Skin that some outward ray leaves without crossing the garment, and that is not in front of a garment face."""
    hit, normal, _, _ = garment.ray_cast(Vector(p), Vector(-n), POKE_DEPTH)
    if hit is not None and np.dot(np.array(normal), n) > 0:
        return False
    # Skin just under the garment pokes through once the idle pose moves it, so it stays hidden.
    if garment.ray_cast(Vector(p + n * .001), Vector(n), SNUG)[0] is not None:
        return False
    if snug(p):
        return False
    return any(garment.ray_cast(Vector(p + n * .001), Vector(d), .5)[0] is None
               for d in [n, *(d for d in VIEW_RAYS if np.dot(d, n) > OPENING_CONE)])


covered = np.array([covers(p, n) for p, n in zip(rest_points, rest_normals)])
# Erosion removed the scattered patches where unpushed skin pokes through, so the face centre is tested instead.
hidden = covered[bf].all(1)
for f in np.flatnonzero(hidden):
    normal = rest_normals[bf[f]].mean(0)
    hidden[f] = covers(rest_points[bf[f]].mean(0), normal / np.linalg.norm(normal))
restored = 0
if SPEC.get('restore_visible'):
    for f in np.flatnonzero(hidden):
        normal = rest_normals[bf[f]].mean(0)
        if seen_through_opening(rest_points[bf[f]].mean(0), normal / np.linalg.norm(normal)):
            hidden[f] = False
            restored += 1
if 'hide_underwear' in SPEC:
    _, welded_body = np.unique(np.round(rest_points, 5), axis=0, return_inverse=True)
    welded_body = welded_body.ravel()
    parent = np.arange(welded_body.max() + 1)

    def root(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for a, b, c in welded_body[bf]:
        parent[root(b)] = parent[root(c)] = root(a)
    islands = np.array([root(i) for i in welded_body])
    skin_island = np.bincount(islands[bf[:, 0]]).argmax()
    low, high = SPEC['hide_underwear']
    for island in np.unique(islands):
        z = rest_points[islands == island, 2]
        if island != skin_island and z.min() > low and z.max() < high:
            hidden |= (islands[bf] == island).all(1)
hide_columns = [i for i, n in enumerate(names) if n.startswith(SPEC.get('hide_bones', ()))]
if hide_columns:
    hidden |= bw[:, hide_columns].sum(1)[bf].min(1) > .5
rim_heights = {}
if heel:
    # Ray coverage missed knee patches, so hide each leg below the lowest point of its boot's top rim.
    for side, sign in (('Left', 1), ('Right', -1)):
        boot = points[points[:, 0] * sign > 0]
        top = boot[boot[:, 2] > boot[:, 2].max() - .08]
        offset = top[:, :2] - top[:, :2].mean(0)
        sector = ((np.arctan2(offset[:, 1], offset[:, 0]) + np.pi) / (2 * np.pi) * 24).astype(int) % 24
        rim = min(top[sector == k, 2].max() for k in np.unique(sector))
        columns = [i for i, n in enumerate(names) if n.startswith(tuple(side + b for b in ('UpLeg', 'Leg', 'Foot', 'Toe')))]
        leg = (bw[:, columns].sum(1) > .5) & (rest_points[:, 0] * sign > 0) & (rest_points[:, 2] < rim - .01)
        hidden |= leg[bf].all(1)
        rim_heights[side] = float(rim)
cuff_planes = {}
if SPEC.get('hide_inside_cuff'):
    # Ray coverage left forearm and hand flecks poking through, so hide skin past the cuff rim that lies against
    # the glove; the fingers out of its open ends lie farther away and stay.
    for side, sign in (('Left', 1), ('Right', -1)):
        centre, normal, _ = cuff_plane(points[points[:, 0] * sign > 0], side)
        arm = (bw[:, forearm_hand_columns(side)].sum(1) > .5) & ((rest_points - centre) @ normal > CUFF_SKIN_INSET)
        arm[arm] = [garment.find_nearest(Vector(p), CUFF_SKIN_GAP)[0] is not None for p in rest_points[arm]]
        hidden |= arm[bf].all(1)
        cuff_planes[side] = {'centre_m': centre.round(4).tolist(), 'normal': normal.round(4).tolist()}
motion_pokes = 0
if not heel:
    # Skin covered at rest can poke through once body and garment deform differently, so pose both and
    # hide skin that lands in front of a garment face in any sampled frame.
    # Only skin the garment covers at rest; elsewhere hiding a frame's poke opened a hole in the other frames.
    centres = rest_points[bf].mean(1)
    face_normals = rest_normals[bf].mean(1)
    face_normals /= np.linalg.norm(face_normals, axis=1, keepdims=True)
    def under_or_against(f):
        if garment.ray_cast(Vector(centres[f] + face_normals[f] * .001), Vector(face_normals[f]), COVER_RAY)[0] is not None:
            return True
        # Skin lying just outside the bodice surface, which the normal rays slip past.
        near, near_normal, _, distance = garment.find_nearest(Vector(centres[f]))
        return distance < .01 and np.dot(centres[f] - np.array(near), np.array(near_normal)) > 0

    candidates = np.array([f for f in np.flatnonzero(~hidden) if under_or_against(f)], dtype=int)
    rig.animation_data_create()
    for clip in MOTION_CLIPS:
        action = bpy.data.actions[clip]
        rig.animation_data.action = action
        for frame in np.linspace(*action.frame_range, MOTION_FRAMES):
            scene.frame_set(int(frame), subframe=float(frame % 1))
            bpy.context.view_layer.update()
            frame_pose = np.array([np.array(rig.matrix_world @ rig.pose.bones[n].matrix @ rig.data.bones[n].matrix_local.inverted()
                                      @ rig.matrix_world.inverted()) for n in names])
            posed_garment = deform(points, order, values, frame_pose)
            posed_tree = BVHTree.FromPolygons([Vector(p) for p in posed_garment], faces.tolist())
            body_matrices = np.einsum('nk,kij->nij', bw, frame_pose)
            posed_body = np.einsum('nij,nj->ni', body_matrices[:, :3, :3], rest_points) + body_matrices[:, :3, 3]
            posed_normals = np.einsum('nij,nj->ni', body_matrices[:, :3, :3], rest_normals)
            for f in candidates[~hidden[candidates]]:
                centre = posed_body[bf[f]].mean(0)
                normal = posed_normals[bf[f]].mean(0)
                normal /= np.linalg.norm(normal)
                hit, face_normal, _, _ = posed_tree.ray_cast(Vector(centre), Vector(-normal), POKE_DEPTH)
                if hit is not None and np.dot(np.array(face_normal), normal) > 0:
                    hidden[f] = True
                    motion_pokes += 1
    rig.animation_data_clear()
    for bone in rig.pose.bones:
        bone.matrix_basis.identity()
    bpy.context.view_layer.update()

if hem_height:
    # Skin below the hem cut is bare once the skirt comes off; arms keep their own rule above.
    face_centres = rest_points[bf].mean(1)
    cut_heights = np.array([hem_height(c) for c in face_centres])
    below = (face_centres[:, 2] < cut_heights - SLEEVE_SKIN_OVERLAP) & (np.abs(face_centres[:, 0]) < .25)
    hidden[below] = False
for side, cut in sleeve_cut.items():
    # Skin past the cut is bare once the gloves come off, so only skin up to the cut stays hidden.
    origin, axis = (np.array(v) for v in cut.pop('plane'))
    past = (rest_points[bf].mean(1) - origin) @ axis > -SLEEVE_SKIN_OVERLAP
    # Arm skin only: the plane also passes the hips, and a side test bared the lower torso there.
    arm = bw[:, forearm_hand_columns(side)].sum(1)[bf].min(1) > .5
    hidden[past & arm] = False
cuff_clips = []
tucked = {}
if 'clip_cuff' in SPEC:
    # A full glove ends in a flat rim: arm skin is cut along the rim's plane, `clip_cuff` metres inside it, instead of by
    # whole triangles, which left notches above the rim and slivers below it. After the motion pokes, which
    # hid skin just outside the rim again.
    for side, sign in (('Left', 1), ('Right', -1)):
        centre, normal, _ = cuff_plane(points[points[:, 0] * sign > 0], side)
        origin = centre + normal * SPEC['clip_cuff']
        depth = (rest_points - origin) @ normal
        columns = forearm_hand_columns(side)
        if SPEC.get('cuff_past_elbow'):
            # The rim sits on the upper arm, whose skin is cut there too.
            columns = columns + [names.index(side + 'Arm')]
        arm = (bw[:, columns].sum(1) > .5)[bf].all(1)
        inside = (depth > 0)[bf]
        hidden[arm & inside.all(1)] = True
        hidden[arm & ~inside.all(1)] = False
        cuff_clips.append({'point': (origin @ TO_GLTF.T).round(5).tolist(), 'normal': (-normal @ TO_GLTF.T).round(5).tolist(),
                           'triangle_indices': np.flatnonzero(arm & inside.any(1) & ~inside.all(1)).tolist()})
if 'clip_sleeve' in SPEC:
    # A sleeve that ends on the bare arm: as at a glove cuff, arm skin is cut along the plane of the sleeve's end,
    # `inset` metres inside it, and hidden up the sleeve from there.
    # `frill`: a white frill hangs from the cuff band and flares off the arm, so the plane is fitted to the frill
    # and set that far into the band from where they meet (negative: into the frill).
    frill = SPEC['clip_sleeve'].get('frill')
    if frill is not None and 'ruffle' not in SPEC['clip_sleeve']:
        pixels = [base_colour_pixels(m) for m in mesh.materials]
        uv = mesh.uv_layers.active.data
        white = np.zeros(len(points), bool)
        for polygon in mesh.polygons:
            u, v = np.mean([uv[i].uv for i in polygon.loop_indices], axis=0)
            if texel(pixels[polygon.material_index], u, v).min() > .6:
                white[list(polygon.vertices)] = True
    for side, sign in (('Left', 1), ('Right', -1)):
        if frill is None:
            axis = bones[side + 'Hand'] - bones[side + 'ForeArm']
            axis /= np.linalg.norm(axis)
            sleeve = points[points[:, 0] * sign > SLEEVE_SIDE]
            along = (sleeve - bones[side + 'ForeArm']) @ axis
            origin = bones[side + 'ForeArm'] + axis * (np.percentile(along, 99) - SPEC['clip_sleeve']['inset'])
        else:
            upper = bones[side + 'ForeArm'] - bones[side + 'Arm']
            upper /= np.linalg.norm(upper)
            offset = points - bones[side + 'Arm']
            reach = offset @ upper
            sleeve = ((points[:, 0] * sign > bones[side + 'Arm'][0] * sign) & (reach > .06)
                      & (np.linalg.norm(offset - reach[:, None] * upper, axis=1) < .12))
            # `ruffle`: the frill is this much of the sleeve's end, where its colour does not set it apart.
            ruffle = SPEC['clip_sleeve'].get('ruffle')
            ring = points[sleeve & (reach > reach[sleeve].max() - ruffle if ruffle else white)]
            assert len(ring) > 30, f'{side} sleeve frill has {len(ring)} vertices'
            centre, axis = fit_plane(ring)
            axis *= np.sign(axis @ upper)
            origin = centre + axis * (np.percentile((ring - centre) @ axis, 3) - frill)
        depth = (rest_points - origin) @ axis
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm') or n.startswith(side + 'Hand')]
        arm = (bw[:, columns].sum(1) > .5)[bf].all(1)
        inside = (depth < 0)[bf]
        hidden[arm & inside.all(1)] = True
        hidden[arm & ~inside.all(1)] = False
        cuff_clips.append({'point': (origin @ TO_GLTF.T).round(5).tolist(), 'normal': (axis @ TO_GLTF.T).round(5).tolist(),
                           'triangle_indices': np.flatnonzero(arm & inside.any(1) & ~inside.all(1)).tolist()})
neck_plane = None
if 'hide_neck' in SPEC:
    # A stand collar closed round the neck: the neck is wider than the collar band at the nape and came through
    # it. Neck skin is cut along the collar's own tilted plane, `level` of the way up the collar, and hidden
    # below it. The face part's neck stands in the same band and is cut by the same plane in the client.
    spec = SPEC['hide_neck']
    collar = points[(np.abs(points[:, 0]) < COLLAR_HALF_WIDTH) & (points[:, 2] > spec['collar'])]
    centre = collar.mean(0)
    normal = np.linalg.svd(collar - centre)[2][2]
    normal *= np.sign(normal[2])
    along = (collar - centre) @ normal
    origin = centre + normal * (along.min() + spec['level'] * np.ptp(along))
    corners = rest_points[bf]
    neck = (corners[:, :, 2] > spec['above']).all(1) & (islands[bf] == skin_island).all(1) \
        & (np.linalg.norm(corners[:, :, :2] - origin[:2], axis=2) < spec['within']).all(1)
    under = (corners - origin) @ normal < 0
    hidden[neck & under.all(1)] = True
    straddling = neck & under.any(1) & ~under.all(1)
    hidden[straddling] = False
    neck_plane = {'point': (origin @ TO_GLTF.T).round(5).tolist(), 'normal': (normal @ TO_GLTF.T).round(5).tolist(),
                  'radius': spec['within'], 'tilt_deg': float(np.degrees(np.arccos(normal[2])))}
    cuff_clips.append({'point': neck_plane['point'], 'normal': neck_plane['normal'],
                       'triangle_indices': np.flatnonzero(straddling).tolist()})
if 'clip_hem' in SPEC:
    # A flat hem or top edge: skin is cut along its plane instead of by whole triangles, whose corners showed as
    # teeth over the cloth. `keep` is the bare side, `within` the half width (m) of the body the edge goes round.
    # The plane is fitted to the edge: a preview fit tilted the bodice hem 2 cm up at the back, and a level cut
    # at its lowest point left that much of the waist missing above the hem.
    up = 1 if SPEC['clip_hem']['keep'] == 'above' else -1
    cloth_ids = np.unique(np.round(points, 5), axis=0, return_inverse=True)[1].ravel()
    uses = {}
    for face in cloth_ids[faces]:
        for i in range(3):
            key = tuple(sorted((face[i], face[(i + 1) % 3])))
            uses[key] = uses.get(key, 0) + 1
    cloth_points = np.zeros((cloth_ids.max() + 1, 3))
    cloth_points[cloth_ids] = points
    open_rim = np.array([(cloth_points[a] + cloth_points[b]) / 2
                         for (a, b), count in uses.items() if count == 1]).reshape(-1, 3)
    # `per_side` fits each leg's edge its own plane: one plane through both stocking tops missed each by its tilt.
    for sign in (1, -1) if SPEC['clip_hem'].get('per_side') else (0,):
        own = cloth_points[cloth_points[:, 0] * sign >= 0]
        height = own[:, 2].max() if up == 1 else own[:, 2].min()
        rim = open_rim[open_rim[:, 0] * sign >= 0]
        rim = rim[(rim[:, 2] - height) * up > -HEM_BAND]
        if not len(rim):
            # A stocking top generated closed has no open edge: the cap that closes it is the edge.
            rim = closed_rim(own, up) if sign else own[(own[:, 2] - height) * up > -HEM_CAP]
        hem_centre, hem_normal = fit_plane(rim)
        hem_normal *= np.sign(hem_normal[2]) * up
        # `shift` moves the cut toward the bare side; negative, the skin runs that far into the cloth.
        hem_centre = hem_centre + hem_normal * SPEC['clip_hem'].get('shift', 0)
        depth = (rest_points - hem_centre) @ hem_normal
        kept = (depth > 0)[bf]
        near = ((np.abs(rest_points[:, 0]) < SPEC['clip_hem']['within']) & (rest_points[:, 0] * sign >= 0))[bf].all(1)
        if 'hide_underwear' in SPEC:
            near &= (islands[bf] == skin_island).all(1)
        straddling = near & kept.any(1) & ~kept.all(1)
        hidden[straddling | (near & kept.all(1) & (depth[bf].max(1) < .03))] = False
        # A triangle on the covered side with a corner on the edge is not cut, and coverage missed two on a thigh.
        hidden[near & ~kept.any(1) & (depth[bf].max(1) > -.002)] = True
        cuff_clips.append({'point': (hem_centre @ TO_GLTF.T).round(5).tolist(),
                           'normal': (hem_normal @ TO_GLTF.T).round(5).tolist(),
                           'triangle_indices': np.flatnonzero(straddling).tolist()})
if 'clip_edge' in SPEC:
    # A curved open edge such as a neckline: each skin triangle it crosses is cut by the plane through its nearest
    # edge segment, square to the cloth there. `above` and `within` (m) pick the edge; skin stays EDGE_OVERLAP inside it.
    spec = SPEC['clip_edge']
    garment_ids = np.unique(np.round(points, 5), axis=0, return_inverse=True)[1].ravel()
    garment_points = np.zeros((garment_ids.max() + 1, 3))
    garment_points[garment_ids] = points
    users = {}
    for face in garment_ids[faces]:
        for i in range(3):
            users.setdefault(tuple(sorted((face[i], face[(i + 1) % 3]))), []).append(face[(i + 2) % 3])
    # Open edges form loops. A short loop is a slit in the cloth, not an opening: the slits beside the neckline
    # trim bared skin through the bodice.
    rims = [edge for edge, opposite in users.items() if len(opposite) == 1]
    loop = np.arange(len(garment_points))

    def loop_of(i):
        while loop[i] != i:
            loop[i] = loop[loop[i]]
            i = loop[i]
        return i

    for a, b in rims:
        loop[loop_of(a)] = loop_of(b)
    lengths = {}
    for a, b in rims:
        lengths[loop_of(a)] = lengths.get(loop_of(a), 0) + np.linalg.norm(garment_points[a] - garment_points[b])
    middles, outward = [], []
    for (a, b), opposite in users.items():
        if len(opposite) != 1 or lengths[loop_of(a)] < EDGE_LOOP:
            continue
        middle = (garment_points[a] + garment_points[b]) / 2
        along = garment_points[b] - garment_points[a]
        along /= np.linalg.norm(along)
        inward = garment_points[opposite[0]] - middle
        inward -= (inward @ along) * along
        # Only rims that open upward: a sleeve band's lower rim bared the skin under the cloth beside it.
        if (middle[2] > spec['above'] and abs(middle[0]) < spec['within'] and np.linalg.norm(inward) > 1e-6
                and -inward[2] / np.linalg.norm(inward) > .3):
            middles.append(middle)
            outward.append(-inward / np.linalg.norm(inward))
    middles, outward = np.array(middles), np.array(outward)
    centres = rest_points[bf].mean(1)
    near = np.flatnonzero((islands[bf] == skin_island).all(1) & (centres[:, 2] > spec['above'] - EDGE_REACH)
                          & (np.abs(centres[:, 0]) < spec['within'] + EDGE_REACH))
    # A large triangle's centre can lie far from an edge its corner crosses, so its corners count too.
    samples = np.concatenate([centres[near][:, None], rest_points[bf[near]]], axis=1)
    distance = np.linalg.norm(samples[:, :, None] - middles[None, None], axis=3).min(1)
    crossed = {}

    def against_cloth(point, normal):
        behind, facing, _, _ = garment.ray_cast(Vector(point), Vector(-normal), POKE_DEPTH)
        return (behind is not None and np.dot(np.array(facing), normal) > 0) or \
            garment.ray_cast(Vector(point + normal * .0005), Vector(normal), EDGE_CLEARANCE)[0] is not None

    for f, edge in zip(near[distance.min(1) < EDGE_REACH], distance.argmin(1)[distance.min(1) < EDGE_REACH]):
        bare = (rest_points[bf[f]] - middles[edge]) @ outward[edge] > -EDGE_OVERLAP
        normal = rest_normals[bf[f]].mean(0)
        normal /= np.linalg.norm(normal)
        # Where the cloth stands off the body the skin just inside the rim shows from above: it stays unless
        # it lies outside the cloth or against it. Cutting it there left a hole in the cleavage.
        if bare.any():
            hidden[f] = False
            inside = rest_points[bf[f]][~bare] * .7 + centres[f] * .3
            if not bare.all() and any(against_cloth(point, normal) for point in inside):
                crossed.setdefault(int(edge), []).append(int(f))
        elif hidden[f] and not against_cloth(centres[f], normal):
            hidden[f] = False
    # A hidden triangle with bare skin on two sides is a notch in the bare skin and joins it, unless it lies
    # against the cloth: joined, those showed as skin patches on the bodice.
    shared = {}
    for f in near:
        for i in range(3):
            shared.setdefault(tuple(sorted((welded_body[bf[f, i]], welded_body[bf[f, (i + 1) % 3]]))), []).append(f)
    beside = {f: [] for f in near}
    for group in shared.values():
        for f in group:
            beside[f] += [other for other in group if other != f]
    for _ in range(2):
        for f in near[hidden[near]]:
            normal = rest_normals[bf[f]].mean(0)
            if sum(not hidden[other] for other in beside[f]) >= 2 and \
                    not against_cloth(centres[f], normal / np.linalg.norm(normal)):
                hidden[f] = False
    # Skin standing a little in front of the cloth at the rim cannot be hidden or cut there without a gap between
    # the skin above and the cloth: it is tucked behind the cloth instead and stays whole.
    for f in near[distance.min(1) < EDGE_REACH]:
        moved = {}
        for v in bf[f]:
            # The nearest point also finds skin beside the rim, where no ray along the normal meets the cloth.
            point, facing, _, gap = garment.find_nearest(Vector(rest_points[v]), EDGE_LIP)
            if point is not None and np.dot(rest_points[v] - np.array(point), np.array(facing)) > 0 \
                    and np.dot(np.array(facing), rest_normals[v]) > 0:
                moved[int(v)] = np.array(point) - np.array(facing) * EDGE_TUCK
            else:
                # Skin in front of a fold of the trim has its nearest cloth point on the fold's far side.
                point, facing, _, _ = garment.ray_cast(Vector(rest_points[v]), Vector(-rest_normals[v]), EDGE_LIP)
                if point is not None and np.dot(np.array(facing), rest_normals[v]) > 0:
                    moved[int(v)] = np.array(point) - rest_normals[v] * EDGE_TUCK
        normal = rest_normals[bf[f]].mean(0)
        normal /= np.linalg.norm(normal)
        hit, facing, _, _ = garment.ray_cast(Vector(centres[f]), Vector(-normal), EDGE_LIP)
        cut = [triangles for triangles in crossed.values() if f in triangles]

        def pokes(v):
            behind, cloth, _, _ = garment.ray_cast(Vector(rest_points[v]), Vector(-rest_normals[v]), POKE_DEPTH)
            return behind is not None and np.dot(np.array(cloth), rest_normals[v]) > 0

        # A cut triangle whose corners all end up behind the cloth needs no cut: the cloth covers it, and the cut
        # opened a hole where the rim stands off the skin.
        if (cut and not any(pokes(v) for v in bf[f] if int(v) not in moved)) or len(moved) > 1 \
                or (moved and hit is not None and np.dot(np.array(facing), normal) > 0):
            tucked.update(moved)
            hidden[f] = False
            for triangles in cut:
                triangles.remove(f)
    for edge, triangles in crossed.items():
        if not triangles:
            continue
        cuff_clips.append({'point': ((middles[edge] - outward[edge] * EDGE_OVERLAP) @ TO_GLTF.T).round(5).tolist(),
                           'normal': (outward[edge] @ TO_GLTF.T).round(5).tolist(), 'triangle_indices': triangles})
brush = json.loads(PREVIEW_FIT.read_text())['parts'].get(PART, {}).get('brush', {}) if PREVIEW_FIT.exists() else {}
hidden[brush.get('hide', [])] = True
hidden[brush.get('show', [])] = False

bpy.ops.object.select_all(action='DESELECT')
mesh_obj.select_set(True)
bpy.context.view_layer.objects.active = mesh_obj
repainted_texels = repaint(SPEC['repaint']) if 'repaint' in SPEC else 0
intermediate = OUTPUT / 'geometry.glb'
bpy.ops.export_scene.gltf(filepath=str(intermediate), export_format='GLB', use_selection=True,
                          export_animations=False, export_skins=False, export_attributes=True, export_tangents=False)
doc, raw = read_glb(intermediate)
raw = bytearray(raw)
remap_order = np.array([joint_names.index(n) for n in names])
for primitive in doc['meshes'][0]['primitives']:
    attrs = primitive['attributes']
    joints = accessor(doc, raw, attrs.pop('_RIG_JOINTS')).astype(np.uint16)
    attrs['JOINTS_0'] = add_accessor(doc, raw, remap_order[joints], 'VEC4', 5123)
    attrs['WEIGHTS_0'] = add_accessor(doc, raw, accessor(doc, raw, attrs.pop('_RIG_WEIGHTS')), 'VEC4')
ids = base['skins'][0]['joints']
remap = {old: new for new, old in enumerate(ids)}
doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in ids]
for node in doc['nodes']:
    if 'children' in node:
        node['children'] = [remap[i] for i in node['children'] if i in remap]
children = {i for node in doc['nodes'] for i in node.get('children', [])}
doc['nodes'].append({'name': 'Armature', 'children': [i for i in range(len(ids)) if i not in children]})
doc['nodes'].append({'name': PART_ID, 'mesh': 0, 'skin': 0, 'extras': {'rig_id': RIG_ID, 'part_id': PART_ID,
                                                                         **({'robe_physics': SPEC['robe']} if SPEC.get('robe') else {})}})
skin = copy.deepcopy(base['skins'][0])
skin['joints'] = list(range(len(ids)))
if 'skeleton' in skin:
    skin['skeleton'] = remap[skin['skeleton']]
skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']), 'MAT4')
doc['skins'] = [skin]
doc['scenes'] = [{'nodes': [len(ids), len(ids) + 1], 'extras': {'rig_id': RIG_ID, **({'heel_pose': heel} if heel else {})}}]
doc['scene'] = 0
doc.pop('animations', None)
doc['buffers'] = [{'byteLength': len(raw)}]
target = OUTPUT / f'{PART_ID}.glb'
write_glb(target, doc, raw)
intermediate.unlink()

body_sha256 = digest(BODY)
mask = {'body': str(BODY.relative_to(ROOT)), 'body_sha256': body_sha256, 'mesh_index': 0, 'primitive_index': 0,
        'primitive_triangles': len(bf), 'hide_triangle_indices': np.flatnonzero(hidden).tolist(),
        'scope': f'Body triangles whose vertices all hit the {PART} within {COVER_RAY * 100:.0f} cm along their normals or lie within {POKE_DEPTH * 100:.0f} cm outside it, corners and centre'
                 + (f', plus triangles weighted over half to {", ".join(SPEC["hide_bones"])} bones.' if hide_columns else '.')}
if cuff_clips:
    mask['clip_planes'] = cuff_clips
if neck_plane:
    mask['neck_plane'] = neck_plane
if tucked:
    # Every copy of a moved vertex across UV seams moves with it.
    copies = {}
    for v, position in tucked.items():
        copies.setdefault(int(welded_body[v]), []).append(position)
    mask['vertex_adjustments'] = [{'vertex': int(v), 'position': (np.mean(copies[int(w)], 0) @ TO_GLTF.T).round(5).tolist(),
                                   'normal': (rest_normals[v] @ TO_GLTF.T).round(5).tolist()}
                                  for v, w in enumerate(welded_body) if int(w) in copies]
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')

mesh_obj.modifiers.new('Female rig', 'ARMATURE').object = rig
mesh_obj.parent = rig
for group in list(mesh_obj.vertex_groups):
    mesh_obj.vertex_groups.remove(group)
for n in names:
    mesh_obj.vertex_groups.new(name=n)
for i in range(len(points)):
    for joint, weight in zip(order[i], values[i]):
        if weight:
            mesh_obj.vertex_groups[int(joint)].add([i], float(weight), 'REPLACE')
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / f'female-{OUTFIT}-{PART}.blend'))


def percentiles(d):
    return {k: round(float(v) * 1000, 2) for k, v in zip(('p1', 'p5', 'median', 'p95'), np.percentile(d, [1, 5, 50, 95]))}


report = {'date': date.today().isoformat(), 'outfit': OUTFIT, 'part': PART, 'source_nodes': SPEC['nodes'],
          'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': SOURCE_SHA256},
          'body': {'path': str(BODY.relative_to(ROOT)), 'sha256': body_sha256},
          'output': {'path': str(target.relative_to(ROOT)), 'sha256': digest(target)},
          'rig_id': RIG_ID, 'scale': SCALE, 'arm_pose': pose.record(),
          **({'heel_lift_m': -foot_shift} if CONFIG['on_heels'] else {}),
          **({'heel_pose_degrees': HEEL_DEGREES, 'leg_hidden_below_rim_m': rim_heights,
              **({} if CONFIG['on_heels'] else {'foot_stretch_m': foot_shift, 'ankle_band_m': ANKLE_BAND})} if heel else {}),
          'triangles': len(faces), 'vertices': len(points), **({'dropped_skin_faces': dropped_skin} if dropped_skin else {}),
          **({'dropped_lining_faces': dropped_lining} if dropped_lining else {}), **({'levelled_tops': levelled_tops} if levelled_tops else {}), **({'repainted_texels': repainted_texels} if repainted_texels else {}), 'skin_distance_mm': percentiles(depths),
          'pushout': False, 'poke_depth_m': POKE_DEPTH,
          'hidden_body_triangles': int(hidden.sum()), 'restored_visible_triangles': restored, 'motion_poke_triangles': motion_pokes,
          'brush_triangles': {'hide': len(brush.get('hide', [])), 'show': len(brush.get('show', []))},
          'preview_fit_adjustments': preview_fits,
          **({'skin_hidden_past_cuff': cuff_planes} if cuff_planes else {}),
          **({'sleeve_axis_shift': sleeve_shift} if sleeve_shift else {}),
          **({'glove_align': glove_align} if glove_align else {}), **({'stocking_taper': stocking_taper} if stocking_taper else {}),
          **({'waist_taper': waist_taper} if waist_taper else {}),
          **({'sleeve_end_offsets_cm': sleeve_ends} if RIGID_SLEEVES else {}),
          **({'sleeve_cut': sleeve_cut} if sleeve_cut else {}),
          **({'hem_cut': hem_cut} if hem_cut else {}),
          **({'heel_extension': heel_extension} if heel_extension else {}),
          **({'rigid_post_vertices': rigid_post_vertices} if 'rigid_post' in SPEC else {}),
          **({'sole_faces': sole_faces, 'sole_colour_srgb': [round(c, 4) for c in sole_colour]} if sole_faces else {}),
          'method': f'Uniform {SCALE} scale' + (', lowered by the heel pose lift' if CONFIG['on_heels'] else '') + ', forearm rotation about the elbow and rigid hand ICP from the generated body pose, '
                    'no push-out (covered and poking skin is hidden), barycentric skin weights from the nearest base skin smoothed over welded neighbours.',
          'status': 'Candidate; visual and animation review pending'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print('REPORT', json.dumps(report))
