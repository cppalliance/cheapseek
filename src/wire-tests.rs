use super::*;

#[test]
fn every_model_is_found_by_its_own_name_and_by_no_other() {
    for model in Model::ALL {
        assert_eq!(Model::from_id(model.id()), Some(model));
    }
    assert_eq!(Model::from_id("cheapseek "), None);
    assert_eq!(Model::from_id("CheapSeek"), None);
    assert_eq!(Model::from_id(""), None);
}

#[test]
fn the_error_envelope_writes_an_absent_param_and_code_as_null() {
    let body = ErrorBody::invalid_request("wrong".to_owned(), None, None);
    let json = serde_json::to_string(&body).expect("the envelope is plain JSON");
    assert_eq!(
        json,
        r#"{"error":{"message":"wrong","type":"invalid_request_error","param":null,"code":null}}"#
    );
}
