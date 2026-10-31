#!/usr/bin/env python3
"""Generate sound-effect candidates with the ElevenLabs SFX API, or short
music with the Music API (--music).

Usage: gen-death-sfx.py [--music [--length S] [--model M] [--format F]]
                        [--takes N] [--duration S] [subject ...]
Key is read from ~/.config/elevenlabs/key (or $ELEVENLABS_API_KEY).
Candidates land in sfx-candidates/<subject>-<n>.mp3 (gitignored).
"""
import argparse, json, os, pathlib, sys, time, urllib.error, urllib.request

PROMPTS = {
    "dungeon_drip": (
        "One isolated water droplet falling into a still shallow puddle of water. "
        "Close-up realistic water foley: a distinct small wet plip, a tiny liquid "
        "splash and a very brief natural bubble tail. Clearly water landing in "
        "water, delicate and organic. Immediate onset, one drop only, then silence. "
        "No solid surface impact, thud, wood, drum, metallic ping, bell, musical "
        "note, synthesizer, electronic beep, flowing water, rain, voice or music."
    ),
    "rain_drops": (
        "Very light rain beginning: sparse tiny raindrops softly tapping leaves "
        "and dry earth. Delicate irregular ticks and pits, distinct individual "
        "droplet impacts with quiet gaps. Natural close-up foley, steady sparse "
        "density, seamless loop, otherwise silent background. No flowing water, "
        "stream, trickling, runoff, gurgling, puddle splashes, continuous wash "
        "or hiss, heavy rain, wind, thunder, birds, voices, music or metallic taps."
    ),
    "metal_hit": (
        'A single steel weapon striking a worn iron breastplate: an immediate hard '
        'dry metallic clang with a short rattling ring of loose armor plates. One '
        'crisp close-up melee hit, under one second. No voice, no music, no rock '
        'debris, no reverb, no repeated hits.'
    ),
    "armored_bone_hit": (
        "One heavy sword hit on a skeleton in battered iron armor. Immediate low, "
        "dense iron thud fused with a dry hollow bone crack and crunch, then a brief "
        "coarse rattle of loose bones and damped armor plates. Weighty, gritty "
        "close-up melee foley, strong low-mid body, tight decay, under one second. "
        "No bright ringing, bell, chime, ping, repeated hits, swoosh, voice, music, "
        "reverb, explosion or bass boom."
    ),
    "orc": "The short, guttural death roar of an orc warrior cut down by a sword in battle",
    "orc_female": "A woman's very short, sharp, high-pitched death scream, a single shrill feminine cry less than half a second long, cut off instantly as a sword strikes her down. Clearly a female human-like voice, high soprano pitch, no growl, no male voice, no music.",
    "hobgoblin": "The harsh, guttural death cry of a hobgoblin soldier felled by a sword: a rough snarling humanoid war-cry choking off as he falls. Voice only, no dog, no barking, no animal, no music.",
    "gnoll": "The yelping, hyena-like death howl of a gnoll cut down in battle",
    "bugbear": "A big shaggy bear-like goblin monster's death cry: a snarling guttural growl breaking into a pained yelp as it is cut down. Animal voice only, no drums, no percussion, no music.",
    "ogre": "A huge brutish ogre monster's deep guttural death groan, a hoarse animal voice choking off as it collapses from a sword wound. Voice only, no horns, no music.",
    "troll": "The drawn-out, rasping death roar of a troll dying from a deep sword wound",
    "stone_golem": "A stone golem crumbling apart, grinding rock and falling rubble as it dies",
    "orc_boss": "The furious, booming death roar of a massive orc chieftain falling in battle",
    "ogre_boss": "A giant ogre warlord monster's short death cry: a deep hoarse beast voice that climbs sharply in pitch at the very end, finishing on a high choked yelp as it falls. Voice only, no horns, no music.",
    "player_death_female": "A young female warrior's short agonized death scream as a sword strikes her down, a single piercing cry cut off abruptly as she falls. Human voice only, no music, no reverb.",
    "player_death_male": "A young male warrior's short agonized death scream as a sword strikes him down, a single hoarse cry cut off abruptly as he falls. Human voice only, no music, no reverb.",
    "sword_stone": "A steel sword blade striking a stone golem: a hard metallic clang ringing off solid rock with a short spray of stone chips and grit. Impact only, no voice, no music.",
    "sword_flesh": "A sword blade cutting deep into a huge fleshy monster: a heavy wet meaty thud with a thick tearing of flesh. Impact only, no voice, no music.",
    "bone_hit": (
        "A single close-up impact on a dry skeleton ribcage: a hard hollow bone clack "
        "followed immediately by a very short brittle rattle of loose bones knocking together. "
        "Tight, dry, crisp game combat hit, under one second. One strike only, "
        "no music, no voice, no flesh, no metal ringing, no reverb."
    ),
    "bone_hit_heavy": (
        "One forceful sword blow into a solid skeleton ribcage. A deep dry hollow "
        "bone thump with a sharp dense crack fused into the attack, followed by a "
        "tiny gritty crunch of broken bone. Heavy, punchy, close-up game combat "
        "foley. Strong low-mid impact, compact tail, immediate onset, one strike "
        "only. No metal, ringing, delicate clicking, loose clacking, flesh, voice, "
        "whoosh, music, reverb or explosion."
    ),
    "skeleton_death": (
        "A dry skeleton suddenly crumbling into a loose pile of bones: an immediate brittle "
        "crack followed by a fast cascading clatter of many small hollow bone fragments, "
        "a crisp papery crunch and skittering rattle as the pieces scatter and settle on "
        "a stone floor. One short continuous collapse, starting immediately and fading "
        "naturally to silence. Close, dry game sound effect, no voice, no music, no flesh, "
        "no metal, no glass, no explosion, no reverb."
    ),
    "terraclash_slam": (
        "A colossal stone giant's fist slamming into packed earth: an immediate "
        "massive low thud of rock on ground, a sharp crack of splitting stone, "
        "then a brief spray of gravel and dirt clods settling. Heavy, deep, "
        "close-up boss impact, one blow only. No metal, voice, roar, music, "
        "whoosh, explosion or reverb."
    ),
    "terraclash_eruption": (
        "Jagged basalt rock spikes bursting violently up through cracked molten "
        "ground: a sudden deep crack, grinding stone shards punching upward, a "
        "short hiss and sizzle of lava, then falling pebbles and dust. Immediate "
        "onset, one eruption, fading to silence. No voice, music, metal, fire "
        "whoosh or explosion boom."
    ),
    "terraclash_roar": (
        "A colossal living rock giant's furious roar: an enormous deep guttural "
        "bellow fused with grinding boulders and rumbling stone, shaking the "
        "ground, with rocks and gravel tumbling down. Starts immediately at full "
        "force, sustained, then fades. A monster voice made of stone. No human "
        "voice, music, dragon screech or thunder."
    ),
    "terraclash_smash": (
        "A gigantic stone fist smashing the ground with earth-shattering force: "
        "an immediate colossal impact with a deep shockwave, the earth splitting "
        "open along long cracks, rock slabs heaving up, heavy debris and clods "
        "raining down. One devastating blow, starting immediately. No voice, "
        "music, metal, fiery explosion or long reverb."
    ),
    "terraclash_quake": (
        "An earthquake building up: a deep low rumble of the ground trembling, "
        "growing steadily louder and stronger, with grinding rock, cracking "
        "earth and rattling pebbles, ending at its most intense. Starts quiet, "
        "no impact at the start. No voice, music, thunder or wind."
    ),
    "terraclash_shell_burst": (
        "A thick rock shell bursting apart as a giant stone creature breaks "
        "free: an immediate violent crack, boulders splitting and tumbling, a "
        "heavy cascade of rubble and gravel, then dust settling. Starts "
        "immediately, one burst. No voice, roar, music, metal, fire or "
        "explosion boom."
    ),
    "terraclash_sleep": (
        "A colossal stone giant slowly grinding down and sinking into the "
        "earth: deep grinding rock, a heavy low rumble, soil and gravel "
        "shifting and pouring, ending as the rock closes over it with a dull "
        "heavy thud. No voice, roar, music, metal or explosion."
    ),
    "terraclash_death": (
        "A colossal living rock giant's death: a deep, weary stony groan "
        "fading out, then its huge body collapsing to the ground, massive "
        "boulders cracking apart and crumbling into an avalanche of rubble "
        "and gravel that slowly settles. No human voice, music, metal, "
        "explosion or thunder."
    ),
    "scp939": "The wet, distorted death shriek of a fleshy eyeless monster, unnatural and wrong",
    "primal_roar_male": (
        "A primal caveman warrior's short fierce battle roar: one deep, raw, "
        "guttural human shout bursting from the chest to frighten wild beasts, "
        "savage and powerful, about one second, ending abruptly. Human male voice "
        "only, no animal growl, no words, no music, no reverb."
    ),
    "primal_roar_female": (
        "A primal cavewoman warrior's short fierce battle roar: one raw, powerful, "
        "guttural female war cry bursting from the chest to frighten wild beasts, "
        "savage and strong, about one second, ending abruptly. Human female voice "
        "only, no scream of fear, no animal growl, no words, no music, no reverb."
    ),
    "healing_prayer_chime": (
        "A gentle holy healing spell for a fantasy RPG: a soft, warm shimmer of "
        "a few quiet celesta and harp notes rising lightly, with a faint airy "
        "sparkle, then fading out smoothly. Calm, soothing, subtle and quiet, "
        "short game heal feedback, about one second. No voice, no choir, no "
        "words, no drums, no bass, no whoosh, no loud bell, no fanfare."
    ),
    "healing_prayer_glow": (
        "A quiet restorative magic glow: a soft breathy shimmer swelling gently "
        "like warm light, with a delicate high glassy twinkle on top, then a "
        "smooth short fade. Gentle priest healing spell in a fantasy game, "
        "understated and calm, about one second. No voice, no choir, no words, "
        "no drums, no bass boom, no whoosh, no harsh ringing."
    ),
    "level_up": (
        "Fantasy RPG level-up sound: a bright magical ascending shimmer. A quick "
        "rising harp and glockenspiel arpeggio with sparkling twinkles and a soft "
        "airy whoosh spiraling upward, ending on a warm radiant chime that fades "
        "gently into glittering sparkles. Uplifting, celebratory, clean and "
        "polished game UI feedback. No voice, no drums, no bass boom, no explosion."
    ),
    "jackpot_fanfare": (
        "Short triumphant fantasy RPG jackpot fanfare: a bright brass and bell "
        "'ta-da, ba-bam' four-note flourish ending on a confident major chord, with "
        "a few sparkling coin jingles on top. Cheerful, punchy game reward sting, "
        "under two seconds. No voice, no drums, no bass boom, no explosion."
    ),
    "jackpot_babam": (
        "Two-hit brass fanfare sting, 'ba-BAM!': a short staccato trumpet stab, "
        "a tiny gap, then a bigger held major chord hit with bright bells and a "
        "sparkle of gold coins ringing out. Exactly two distinct accents, the "
        "second louder and longer. Triumphant game jackpot reward, under two "
        "seconds. No voice, no drums, no bass boom, no explosion, no third hit."
    ),
    "jackpot_tada": (
        "Classic 'ta-daa!' victory sting: two quick ascending orchestral brass "
        "notes, a short pickup note then a strong sustained high note with "
        "cymbal-free bell shimmer and jingling coins. Clear two-note rhythm, "
        "cheerful fantasy game jackpot, under two seconds. No voice, no drums, "
        "no bass, no explosion."
    ),
    "jackpot_chime": (
        "Bright magical treasure-found chime: a quick sparkling 'tee-ree-ring' "
        "glockenspiel and celesta flourish rising to a shining bell note, layered "
        "with a shimmering shower of gold coins. Joyful game UI reward sound, "
        "under two seconds. No voice, no drums, no bass, no whoosh."
    ),
    "key_drop_mystery": (
        "A mysterious magical item appearing in a dark dungeon: a soft eerie "
        "'dee-ree-ring' music-box and celesta phrase in a minor key, with a "
        "faint crystalline shimmer and a light metallic key tinkle at the end. "
        "Enchanted, curious and slightly ominous, short game discovery sting. "
        "No voice, no drums, no bass boom, no whoosh."
    ),
    "key_drop_arcane": (
        "Secret discovered in an ancient crypt: a short ghostly glassy chime "
        "arpeggio with an unresolved mystical interval, wrapped in a quiet "
        "shimmering reverb tail, and one small iron key clinking onto stone. "
        "Mysterious, magical, attention-grabbing but subtle game sound. No "
        "voice, no drums, no bass, no horror scream."
    ),
}

# --length sets music_length_ms; a 180 s music_v2_5 take was cut off mid-phrase at that mark.
MUSIC_PROMPTS = {
    "battle_hymn_march": (
        "A rousing medieval fantasy battle march played by a bard as an army "
        "sets out to war. Pounding war drums and a crisp military snare drive a "
        "steady 120 BPM march from the very first beat, with a bright plucked "
        "mandolin playing a heroic fanfare melody on top. Bold, courageous and "
        "building. It ends on one strong final drum hit and chord at about nine "
        "seconds, then a brief ring-out. Instrumental only: no vocals, no choir."
    ),
    "hearthside_tavern": (
        "Warm and relaxed medieval tavern music. Gentle lute fingerpicking sets a "
        "cozy mood. Soft fiddle plays a slow, sweet melody. Harp adds delicate "
        "touches. Feels like resting by the fireplace with a mug of ale after a "
        "long journey. Calm and inviting. Slow, gentle tempo around 84 BPM in "
        "B-flat major. Warm, mellow, intimate acoustic sound with soft rounded "
        "highs. Steady, gentle dynamics from start to finish with no big climax; "
        "no drums, at most a very soft hand drum. A simple memorable melody that "
        "returns with small variations. Ends softly on a resolved chord so it can "
        "loop back to the beginning. Instrumental only, no vocals."
    ),
}

API = "https://api.elevenlabs.io/v1/sound-generation"
MUSIC_API = "https://api.elevenlabs.io/v1/music?output_format="
SUB = "https://api.elevenlabs.io/v1/user/subscription"


def key():
    k = os.environ.get("ELEVENLABS_API_KEY")
    if k:
        return k.strip()
    p = pathlib.Path.home() / ".config/elevenlabs/key"
    if p.exists():
        return p.read_text().strip()
    sys.exit("no API key: set $ELEVENLABS_API_KEY or write ~/.config/elevenlabs/key")


def credits(k):
    """Remaining credits, or None if the key lacks the user_read permission."""
    req = urllib.request.Request(SUB, headers={"xi-api-key": k})
    try:
        with urllib.request.urlopen(req) as r:
            d = json.load(r)
    except urllib.error.HTTPError:
        return None
    return d["character_limit"] - d["character_count"]


def post(k, url, params):
    req = urllib.request.Request(
        url,
        data=json.dumps(params).encode(),
        headers={"xi-api-key": k, "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req) as r:
            return r.read()
    except urllib.error.HTTPError as e:
        sys.exit(f"HTTP {e.code}: {e.read().decode(errors='replace')[:500]}")


def generate(k, text, duration, influence, loop=False):
    params = {"text": text, "duration_seconds": duration, "prompt_influence": influence}
    if loop:
        params.update(model_id="eleven_text_to_sound_v2", loop=True)
    return post(k, API, params)


def compose(k, prompt, length, model, fmt):
    params = {"prompt": prompt, "model_id": model, "force_instrumental": True}
    if length:
        params["music_length_ms"] = int(length * 1000)
    return post(k, MUSIC_API + fmt, params)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("subjects", nargs="*")
    ap.add_argument("--takes", type=int, default=3)
    ap.add_argument("--duration", type=float, default=1.5)
    ap.add_argument("--influence", type=float, default=0.4)
    ap.add_argument("--loop", action="store_true")
    ap.add_argument("--music", action="store_true")
    ap.add_argument("--length", type=float)
    ap.add_argument("--model", default="music_v1")
    ap.add_argument("--format", default="mp3_44100_128")
    ap.add_argument("--out", default="sfx-candidates")
    args = ap.parse_args()

    prompts = MUSIC_PROMPTS if args.music else PROMPTS
    targets = args.subjects or list(prompts)
    unknown = [m for m in targets if m not in prompts]
    if unknown:
        sys.exit(f"no prompt for: {', '.join(unknown)}")

    k = key()
    before = credits(k)
    print(f"credits left: {before}" if before is not None else "credits: n/a (key lacks user_read)")
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    generated = 0
    for m in targets:
        for n in range(1, args.takes + 1):
            dest = out / f"{m}-{n}.mp3"
            if dest.exists():
                print(f"skip {dest}")
                continue
            if args.music:
                audio = compose(k, prompts[m], args.length, args.model, args.format)
            else:
                audio = generate(k, prompts[m], args.duration, args.influence, args.loop)
            dest.write_bytes(audio)
            generated += 1
            print(f"{dest}  {len(audio)} bytes")

    if generated and before is not None:
        time.sleep(20)  # usage reporting lags a little behind generation
        after = credits(k)
        if after is not None:
            print(f"credits left: {after}  (used {before - after})")


if __name__ == "__main__":
    main()
