//! Hair salon: changing a character's look after creation, priced by what changed.

use serde::{Deserialize, Serialize};

use crate::character::CharacterAppearance;

/// Registry id of the NPC who restyles characters.
pub const HAIRDRESSER: &str = "hairdresser";

/// Copper.
pub const FACE_PRICE: i64 = 10_000;
pub const HAIR_PRICE: i64 = 5_000;
pub const HAIR_COLOR_PRICE: i64 = 2_000;
pub const EYE_COLOR_PRICE: i64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalonError {
    TooFar,
    Unsupported,
    Unchanged,
    Trading,
    NotEnoughGold,
    Failed,
}

/// What changing `from` into `to` costs; a colour and its "original colour" switch count as one part.
pub fn price(from: &CharacterAppearance, to: &CharacterAppearance) -> i64 {
    let part = |changed: bool, price: i64| if changed { price } else { 0 };
    part(from.face != to.face, FACE_PRICE)
        + part(from.hair != to.hair, HAIR_PRICE)
        + part(
            (from.hair_color, from.hair_color_original) != (to.hair_color, to.hair_color_original),
            HAIR_COLOR_PRICE,
        )
        + part(
            (from.eye_color, from.eye_color_original) != (to.eye_color, to.eye_color_original),
            EYE_COLOR_PRICE,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{FaceStyle, HairStyle};

    #[test]
    fn each_changed_part_is_charged_once() {
        let from = CharacterAppearance::default();
        assert_eq!(price(&from, &from), 0);
        let face = CharacterAppearance {
            face: FaceStyle::Rugged,
            ..from
        };
        assert_eq!(price(&from, &face), FACE_PRICE);
        let hair = CharacterAppearance {
            hair: HairStyle::WavyBone,
            ..from
        };
        assert_eq!(price(&from, &hair), HAIR_PRICE);
        let original_hair = CharacterAppearance {
            hair_color_original: true,
            ..from
        };
        assert_eq!(price(&from, &original_hair), HAIR_COLOR_PRICE);
        let eyes: CharacterAppearance =
            serde_json::from_str(r##"{"eye_color":"#112233","eye_color_original":true}"##).unwrap();
        assert_eq!(price(&from, &eyes), EYE_COLOR_PRICE);
        let all = CharacterAppearance {
            face: FaceStyle::Rugged,
            hair: HairStyle::WavyBone,
            hair_color_original: true,
            ..eyes
        };
        assert_eq!(
            price(&from, &all),
            FACE_PRICE + HAIR_PRICE + HAIR_COLOR_PRICE + EYE_COLOR_PRICE
        );
    }
}
