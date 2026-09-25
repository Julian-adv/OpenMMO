use onlinerpg_shared::messages::{
    deserialize_server_msg, serialize_server_msg, LocalizedMessage, ServerMessage,
};

#[test]
fn localized_system_message_keeps_english_and_params_on_the_wire() {
    let localization = LocalizedMessage::new("server.goldPickedUp").with_param("amount", "12");
    let message = ServerMessage::SystemMessage {
        message: "You picked up 12 copper.".into(),
        localization: Some(localization.clone()),
    };
    let decoded = deserialize_server_msg(&serialize_server_msg(&message).unwrap()).unwrap();
    match decoded {
        ServerMessage::SystemMessage {
            message,
            localization: actual,
        } => {
            assert_eq!(message, "You picked up 12 copper.");
            assert_eq!(actual, Some(localization));
        }
        _ => panic!("Wrong message kind"),
    }
    let json = serde_json::to_value(&message).unwrap();
    assert_eq!(
        json["SystemMessage"]["localization"]["params"]["amount"],
        "12"
    );
}

#[test]
fn trade_error_keeps_its_event_kind_and_optional_localization() {
    for localization in [None, Some(LocalizedMessage::new("server.insufficientGold"))] {
        let message = ServerMessage::TradeError {
            message: "Not enough gold".into(),
            localization: localization.clone(),
        };
        let decoded = deserialize_server_msg(&serialize_server_msg(&message).unwrap()).unwrap();
        match decoded {
            ServerMessage::TradeError {
                message,
                localization: actual,
            } => {
                assert_eq!(message, "Not enough gold");
                assert_eq!(actual, localization);
            }
            _ => panic!("Wrong message kind"),
        }
    }
}
