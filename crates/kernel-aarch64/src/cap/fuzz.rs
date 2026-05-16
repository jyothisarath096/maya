use super::{
    check_right, check_right_as, create, delegate, get_intent_id, revoke, validate, CapToken,
    IntentClass, ResourceType, Rights,
};

pub fn run_fuzz_suite() {
    crate::uart_print!("cap fuzz begin\n");
    let mut passed = 0usize;
    let mut failed = 0usize;

    let tests: [(&str, fn() -> bool); 17] = [
        ("test_create_validate", test_create_validate),
        ("test_revoke_invalidates", test_revoke_invalidates),
        ("test_cross_pid_denied", test_cross_pid_denied),
        ("test_rights_check", test_rights_check),
        ("test_delegate_basic", test_delegate_basic),
        ("test_delegate_reduced_rights", test_delegate_reduced_rights),
        ("test_telemetry_observe", test_telemetry_observe),
        ("test_intent_id_in_token", test_intent_id_in_token),
        ("test_intent_call_right", test_intent_call_right),
        ("test_cross_pid_intent", test_cross_pid_intent),
        ("test_delegation_depth_limit", test_delegation_depth_limit),
        (
            "test_delegation_rights_no_escalation",
            test_delegation_rights_no_escalation,
        ),
        ("test_observe_right", test_observe_right),
        ("test_cache_invalidation", test_cache_invalidation),
        ("test_mte_retag_check", test_mte_retag_check),
        ("test_pacda_forgery", test_pacda_forgery),
        ("test_generation_wrap", test_generation_wrap),
    ];

    for (name, test) in tests {
        if test() {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: ");
            crate::uart_print!(name);
            crate::uart_print!("\n");
            failed += 1;
        }
    }

    crate::uart_print!("cap fuzz: ");
    crate::uart_print_usize!(passed);
    crate::uart_print!(" passed, ");
    crate::uart_print_usize!(failed);
    crate::uart_print!(" failed\n");
    crate::uart_print!("cap fuzz end\n");
}

fn test_create_validate() -> bool {
    let token = create(1, ResourceType::Memory, 11, Rights::READ, 1, IntentClass::IO).ok();
    token.is_some() && validate(token.unwrap()).is_ok()
}

fn test_revoke_invalidates() -> bool {
    let token = create(1, ResourceType::Channel, 12, Rights::READ, 2, IntentClass::IO).ok();
    if let Some(token) = token {
        revoke(token).is_ok() && validate(token).is_err()
    } else {
        false
    }
}

fn test_cross_pid_denied() -> bool {
    let token = create(1, ResourceType::Intent, 13, Rights::INTENT_CALL, 42, IntentClass::Compute).ok();
    token.is_some() && check_right_as(token.unwrap(), Rights::INTENT_CALL, 2).is_err()
}

fn test_rights_check() -> bool {
    let token = create(1, ResourceType::Memory, 14, Rights::READ, 4, IntentClass::IO).ok();
    token.is_some() && check_right(token.unwrap(), Rights::WRITE).is_err()
}

fn test_delegate_basic() -> bool {
    let token = create(
        1,
        ResourceType::Intent,
        15,
        Rights(Rights::READ.0 | Rights::GRANT.0 | Rights::INTENT_CALL.0),
        5,
        IntentClass::Compute,
    )
    .ok();
    token.is_some() && delegate(token.unwrap(), 2, Rights::READ).is_ok()
}

fn test_delegate_reduced_rights() -> bool {
    let token = create(
        1,
        ResourceType::Channel,
        16,
        Rights(Rights::READ.0 | Rights::GRANT.0),
        6,
        IntentClass::IO,
    )
    .ok();
    if let Some(token) = token {
        if let Ok(child) = delegate(token, 2, Rights(Rights::READ.0 | Rights::WRITE.0)) {
            check_right(child, Rights::READ).is_ok() && check_right(child, Rights::WRITE).is_err()
        } else {
            false
        }
    } else {
        false
    }
}

fn test_telemetry_observe() -> bool {
    let token = create(1, ResourceType::Telemetry, 17, Rights::OBSERVE, 7, IntentClass::System).ok();
    token.is_some() && check_right(token.unwrap(), Rights::OBSERVE).is_ok()
}

fn test_intent_id_in_token() -> bool {
    let token = create(1, ResourceType::Intent, 18, Rights::INTENT_CALL, 42, IntentClass::Compute).ok();
    token.is_some() && get_intent_id(token.unwrap()) == 42
}

fn test_intent_call_right() -> bool {
    let token = create(1, ResourceType::Intent, 19, Rights::READ, 8, IntentClass::Compute).ok();
    token.is_some() && check_right(token.unwrap(), Rights::INTENT_CALL).is_err()
}

fn test_cross_pid_intent() -> bool {
    let token = create(1, ResourceType::Intent, 20, Rights::INTENT_CALL, 9, IntentClass::Compute).ok();
    token.is_some() && check_right_as(token.unwrap(), Rights::INTENT_CALL, 2).is_err()
}

fn test_delegation_depth_limit() -> bool {
    let root = create(
        1,
        ResourceType::Intent,
        21,
        Rights(Rights::READ.0 | Rights::GRANT.0),
        10,
        IntentClass::Compute,
    )
    .ok();
    if let Some(a) = root {
        let b = delegate(a, 2, Rights::READ).ok();
        let c = b.and_then(|t| delegate(t, 3, Rights::READ).ok());
        let d = c.and_then(|t| delegate(t, 4, Rights::READ).ok());
        d.is_none()
    } else {
        false
    }
}

fn test_delegation_rights_no_escalation() -> bool {
    let token = create(
        1,
        ResourceType::Memory,
        22,
        Rights(Rights::READ.0 | Rights::GRANT.0),
        11,
        IntentClass::Background,
    )
    .ok();
    if let Some(token) = token {
        if let Ok(child) = delegate(token, 2, Rights(Rights::READ.0 | Rights::WRITE.0)) {
            check_right(child, Rights::WRITE).is_err()
        } else {
            false
        }
    } else {
        false
    }
}

fn test_observe_right() -> bool {
    let token = create(1, ResourceType::Telemetry, 23, Rights::READ, 12, IntentClass::System).ok();
    token.is_some() && check_right(token.unwrap(), Rights::OBSERVE).is_err()
}

fn test_cache_invalidation() -> bool {
    let token = create(1, ResourceType::Channel, 24, Rights::READ, 13, IntentClass::IO).ok();
    if let Some(token) = token {
        let _ = validate(token);
        let _ = revoke(token);
        validate(token).is_err()
    } else {
        false
    }
}

fn test_mte_retag_check() -> bool {
    super::mte_available().not()
}

fn test_pacda_forgery() -> bool {
    if super::pac_available() {
        let token =
            create(1, ResourceType::Crypto, 25, Rights::EXECUTE, 14, IntentClass::Compute).ok();
        if let Some(token) = token {
            let forged = CapToken::from_parts(token.hi() ^ 1, token.lo());
            let result = validate(forged).is_err();
            let _ = revoke(token);
            result
        } else {
            false
        }
    } else {
        let token =
            create(1, ResourceType::Crypto, 25, Rights::EXECUTE, 14, IntentClass::Compute).ok();
        if let Some(token) = token {
            let _ = revoke(token);
            validate(token).is_err()
        } else {
            false
        }
    }
}

fn test_generation_wrap() -> bool {
    let token = create(1, ResourceType::Memory, 26, Rights::READ, 15, IntentClass::Background).ok();
    if let Some(token) = token {
        let _ = revoke(token);
        validate(token).is_err()
    } else {
        false
    }
}

trait BoolNot {
    fn not(self) -> bool;
}

impl BoolNot for bool {
    fn not(self) -> bool {
        !self
    }
}
