// SPDX-License-Identifier: Apache-2.0
#![cfg(test)]

use super::*;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, AuthorizedFunction, Events as _, Ledger};
use soroban_sdk::{symbol_short, vec, IntoVal, Symbol, Val, Vec};

fn setup() -> (Env, NgoRegistryClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    // Leave room for the 90-day NGO bump so the TTL tests see it unclamped.
    env.ledger()
        .with_mut(|l| l.max_entry_ttl = 365 * DAY_IN_LEDGERS);

    let admin = Address::generate(&env);
    let contract_id = env.register(NgoRegistry, NgoRegistryArgs::__constructor(&admin));
    let client = NgoRegistryClient::new(&env, &contract_id);

    (env, client, admin)
}

#[test]
fn init_sets_admin() {
    let (_env, client, admin) = setup();
    assert_eq!(client.admin(), admin);
}

#[test]
fn double_init_fails() {
    let (_env, client, admin) = setup();
    let result = client.try_init(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
#[should_panic]
fn init_without_admin_auth_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(NgoRegistry, NgoRegistryArgs::__constructor(&admin));
    let client = NgoRegistryClient::new(&env, &contract_id);

    let attacker = Address::generate(&env);
    env.mock_auths(&[]);
    client.init(&attacker);
}

#[test]
fn register_ngo_stores_unverified_entry() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");

    client.register(&owner, &name);

    let ngo = client.get_ngo(&owner);
    assert_eq!(ngo.owner, owner);
    assert_eq!(ngo.name, name);
    assert!(!ngo.verified);
}

#[test]
fn register_whitespace_only_name_fails() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let whitespace = String::from_str(&env, " \t\n\r");

    let result = client.try_register(&owner, &whitespace);

    assert_eq!(result, Err(Ok(Error::InvalidName)));
    assert_eq!(client.try_get_ngo(&owner), Err(Ok(Error::NotRegistered)));
}

#[test]
fn unverified_ngo_can_unregister_and_register_again() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Pending NGO");

    client.register(&owner, &name);
    client.unregister(&owner);
    assert_eq!(client.try_get_ngo(&owner), Err(Ok(Error::NotRegistered)));

    client.register(&owner, &String::from_str(&env, "Updated NGO"));
    assert_eq!(
        client.get_ngo(&owner).name,
        String::from_str(&env, "Updated NGO")
    );
}

#[test]
fn double_register_fails() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");

    client.register(&owner, &name);
    let result = client.try_register(&owner, &name);

    assert_eq!(result, Err(Ok(Error::AlreadyRegistered)));
}

#[test]
fn register_rejects_name_over_max_length() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    // MAX_NGO_NAME_LEN (200) + 1 bytes.
    let too_long = String::from_str(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let result = client.try_register(&owner, &too_long);

    assert_eq!(result, Err(Ok(Error::NameTooLong)));
    assert_eq!(client.try_get_ngo(&owner), Err(Ok(Error::NotRegistered)));
}

#[test]
fn register_accepts_name_at_max_length() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    // Exactly MAX_NGO_NAME_LEN (200) bytes.
    let at_limit = String::from_str(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    client.register(&owner, &at_limit);

    assert_eq!(client.get_ngo(&owner).name, at_limit);
}

#[test]
fn update_name_rejects_name_over_max_length() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    let too_long = String::from_str(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let result = client.try_update_name(&owner, &too_long);

    assert_eq!(result, Err(Ok(Error::NameTooLong)));
}

#[test]
fn update_name_rejects_empty_name() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let original = String::from_str(&env, "Red Cross");
    client.register(&owner, &original);

    let result = client.try_update_name(&owner, &String::from_str(&env, ""));

    assert_eq!(result, Err(Ok(Error::InvalidName)));
    // The rejected rename leaves the stored name alone.
    assert_eq!(client.get_ngo(&owner).name, original);
}

#[test]
fn update_name_rejects_whitespace_only_name() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let original = String::from_str(&env, "Red Cross");
    client.register(&owner, &original);

    let result = client.try_update_name(&owner, &String::from_str(&env, " \t\n\r"));

    assert_eq!(result, Err(Ok(Error::InvalidName)));
    assert_eq!(client.get_ngo(&owner).name, original);
}

#[test]
fn update_name_rejects_blank_name_without_publishing_an_event() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));

    assert_eq!(
        client.try_update_name(&owner, &String::from_str(&env, "   ")),
        Err(Ok(Error::InvalidName))
    );

    // A rejected rename must not publish a renamed event for indexers to pick up.
    assert_eq!(env.events().all(), vec![&env]);
}

#[test]
fn get_unregistered_ngo_fails() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    let result = client.try_get_ngo(&random);
    assert_eq!(result, Err(Ok(Error::NotRegistered)));
}

#[test]
fn is_verified_returns_false_for_registered_unverified_ngo() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));

    assert!(!client.is_verified(&owner));
}

#[test]
fn is_verified_returns_true_for_verified_ngo() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    client.approve_ngo(&owner);

    assert!(client.is_verified(&owner));
}

#[test]
fn is_verified_returns_false_for_unknown_address() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    assert!(!client.is_verified(&random));
}

#[test]
fn is_verified_returns_false_for_revoked_ngo() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    client.approve_ngo(&owner);
    assert!(client.is_verified(&owner));

    client.revoke_ngo(&owner);
    assert!(!client.is_verified(&owner));
}

#[test]
fn approve_ngo_marks_verified() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);

    client.approve_ngo(&owner);

    let ngo = client.get_ngo(&owner);
    assert!(ngo.verified);
}

#[test]
fn approve_ngo_twice_fails() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);

    client.approve_ngo(&owner);

    let result = client.try_approve_ngo(&owner);
    assert_eq!(result, Err(Ok(Error::AlreadyVerified)));
    assert!(client.get_ngo(&owner).verified);
}

#[test]
fn approve_unregistered_ngo_fails() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    let result = client.try_approve_ngo(&random);
    assert_eq!(result, Err(Ok(Error::NotRegistered)));
}

#[test]
fn revoke_ngo_clears_verified_status() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);

    client.approve_ngo(&owner);
    assert!(client.get_ngo(&owner).verified);

    client.revoke_ngo(&owner);

    let ngo = client.get_ngo(&owner);
    assert!(!ngo.verified);
}

#[test]
fn revoke_unverified_ngo_fails() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);

    let result = client.try_revoke_ngo(&owner);
    assert_eq!(result, Err(Ok(Error::NotVerified)));
    assert!(!client.get_ngo(&owner).verified);
}

#[test]
fn revoke_unregistered_ngo_fails() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    let result = client.try_revoke_ngo(&random);
    assert_eq!(result, Err(Ok(Error::NotRegistered)));
}

#[test]
fn full_lifecycle_approve_revoke_reapprove() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");

    // 1. Register NGO
    client.register(&owner, &name);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("register"), owner.clone()).into_val(&env),
                name.clone().into_val(&env),
            ),
        ]
    );
    let ngo = client.get_ngo(&owner);
    assert_eq!(ngo.owner, owner);
    assert_eq!(ngo.name, name);
    assert!(!ngo.verified);

    // 2. Approve NGO
    client.approve_ngo(&owner);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("approved"), owner.clone()).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
    let ngo = client.get_ngo(&owner);
    assert_eq!(ngo.owner, owner);
    assert_eq!(ngo.name, name);
    assert!(ngo.verified);

    // 3. Revoke NGO
    client.revoke_ngo(&owner);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("revoked"), owner.clone()).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
    let ngo = client.get_ngo(&owner);
    assert_eq!(ngo.owner, owner);
    assert_eq!(ngo.name, name);
    assert!(!ngo.verified);

    // 4. Reapprove NGO
    client.approve_ngo(&owner);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("approved"), owner.clone()).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
    let ngo = client.get_ngo(&owner);
    assert_eq!(ngo.owner, owner);
    assert_eq!(ngo.name, name);
    assert!(ngo.verified);
}

#[test]
fn touch_ngo_leaves_entry_unchanged() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);
    client.approve_ngo(&owner);

    let before = client.get_ngo(&owner);
    client.touch_ngo(&owner);
    let after = client.get_ngo(&owner);

    assert_eq!(before, after);
}

#[test]
fn touch_unregistered_ngo_fails() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    let result = client.try_touch_ngo(&random);
    assert_eq!(result, Err(Ok(Error::NotRegistered)));
}

fn instance_ttl(env: &Env, client: &NgoRegistryClient) -> u32 {
    env.as_contract(&client.address, || env.storage().instance().get_ttl())
}

fn ngo_ttl(env: &Env, client: &NgoRegistryClient, owner: &Address) -> u32 {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Ngo(owner.clone()))
    })
}

/// Moves the ledger forward two days, which drops both the instance and a
/// freshly bumped NGO entry below their lifetime thresholds. Asserts that
/// it did, so a test calling this can't pass just because nothing needed
/// a bump.
fn age_past_thresholds(env: &Env, client: &NgoRegistryClient, owner: &Address) {
    env.ledger()
        .with_mut(|l| l.sequence_number += 2 * DAY_IN_LEDGERS);
    assert!(instance_ttl(env, client) < INSTANCE_LIFETIME_THRESHOLD);
    assert!(ngo_ttl(env, client, owner) < NGO_LIFETIME_THRESHOLD);
}

fn assert_ttls_bumped(env: &Env, client: &NgoRegistryClient, owner: &Address) {
    assert_eq!(instance_ttl(env, client), INSTANCE_BUMP_AMOUNT);
    assert_eq!(ngo_ttl(env, client, owner), NGO_BUMP_AMOUNT);
}

#[test]
fn register_bumps_instance_and_ngo_ttl() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));

    assert_ttls_bumped(&env, &client, &owner);
}

#[test]
fn approve_ngo_bumps_instance_and_ngo_ttl() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    age_past_thresholds(&env, &client, &owner);

    client.approve_ngo(&owner);

    assert_ttls_bumped(&env, &client, &owner);
}

#[test]
fn revoke_ngo_bumps_instance_and_ngo_ttl() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    client.approve_ngo(&owner);
    age_past_thresholds(&env, &client, &owner);

    client.revoke_ngo(&owner);

    assert_ttls_bumped(&env, &client, &owner);
}

#[test]
fn update_name_changes_name_before_approval() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Crsos"));

    let fixed = String::from_str(&env, "Red Cross");
    client.update_name(&owner, &fixed);

    // Events cover only the latest top-level call, so read them before
    // `get_ngo` below replaces them.
    let events = env.events().all();
    let expected: Vec<(Address, Vec<Val>, Val)> = vec![
        &env,
        (
            client.address.clone(),
            (symbol_short!("renamed"), owner.clone()).into_val(&env),
            fixed.into_val(&env),
        ),
    ];
    assert_eq!(events, expected);

    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("renamed"), owner.clone()).into_val(&env),
                fixed.into_val(&env)
            ),
        ]
    );

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("renamed"), owner.clone()).into_val(&env),
                fixed.into_val(&env),
            ),
        ]
    );

    assert_eq!(
        client.get_ngo(&owner),
        Ngo {
            owner: owner.clone(),
            name: fixed.clone(),
            verified: false,
        }
    );
}

#[test]
fn update_name_after_approval_fails() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");
    client.register(&owner, &name);
    client.approve_ngo(&owner);

    let result = client.try_update_name(&owner, &String::from_str(&env, "Blue Cross"));
    assert_eq!(result, Err(Ok(Error::AlreadyVerified)));
    assert_eq!(client.get_ngo(&owner).name, name);
}

#[test]
fn update_name_after_revocation_succeeds() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Crsos"));
    client.approve_ngo(&owner);

    // The name an admin approved is locked only while the NGO stays
    // verified, so the rename has to be rejected at this point.
    let blocked = client.try_update_name(&owner, &String::from_str(&env, "Red Cross"));
    assert_eq!(blocked, Err(Ok(Error::AlreadyVerified)));

    client.revoke_ngo(&owner);
    assert!(!client.get_ngo(&owner).verified);

    let fixed = String::from_str(&env, "Red Cross");
    client.update_name(&owner, &fixed);

    // Events cover only the latest top-level call, so read them before
    // `get_ngo` below replaces them.
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("renamed"), owner.clone()).into_val(&env),
                fixed.clone().into_val(&env),
            ),
        ]
    );

    assert_eq!(
        client.get_ngo(&owner),
        Ngo {
            owner: owner.clone(),
            name: fixed,
            verified: false,
        }
    );
}

#[test]
fn update_name_for_unregistered_ngo_fails() {
    let (env, client, _admin) = setup();
    let random = Address::generate(&env);

    let result = client.try_update_name(&random, &String::from_str(&env, "Red Cross"));
    assert_eq!(result, Err(Ok(Error::NotRegistered)));
}

#[test]
fn update_name_requires_owner_auth() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Crsos"));

    client.update_name(&owner, &String::from_str(&env, "Red Cross"));

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    let (address, invocation) = &auths[0];
    assert_eq!(address, &owner);
    match &invocation.function {
        AuthorizedFunction::Contract((contract, function, _)) => {
            assert_eq!(contract, &client.address);
            assert_eq!(function, &Symbol::new(&env, "update_name"));
        }
        _ => panic!("expected a contract invocation"),
    }
}

#[test]
fn update_name_bumps_instance_and_ngo_ttl() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Crsos"));
    age_past_thresholds(&env, &client, &owner);

    client.update_name(&owner, &String::from_str(&env, "Red Cross"));

    assert_ttls_bumped(&env, &client, &owner);
}

#[test]
fn touch_ngo_bumps_instance_and_ngo_ttl() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    age_past_thresholds(&env, &client, &owner);

    client.touch_ngo(&owner);

    assert_ttls_bumped(&env, &client, &owner);
}

// --- Event assertion tests (issue #62) ---

#[test]
fn register_publishes_event() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Red Cross");

    client.register(&owner, &name);

    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("register"), owner).into_val(&env),
                name.into_val(&env),
            ),
        ]
    );
}

#[test]
fn approve_ngo_publishes_event() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));

    client.approve_ngo(&owner);

    // Events are cleared per top-level call; only the approved event is visible.
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("approved"), owner).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
}

#[test]
fn revoke_ngo_publishes_event() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Red Cross"));
    client.approve_ngo(&owner);

    client.revoke_ngo(&owner);

    // Events are cleared per top-level call; only the revoked event is visible.
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (symbol_short!("revoked"), owner).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
}

// =============================================================================
// upgrade (contract bytecode upgrade)
// =============================================================================

#[test]
#[ignore = "requires an uploaded wasm hash; upgrade panics before auth is observable"]
fn upgrade_requires_admin_auth() {
    let (env, client, admin) = setup();
    let fake_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.upgrade(&fake_hash);
    let auths = env.auths();
    assert_eq!(auths.len(), 1, "expected exactly one authorizer");
    assert_eq!(auths[0].0, admin);
    match &auths[0].1.function {
        AuthorizedFunction::Contract((contract, function, _)) => {
            assert_eq!(contract, &client.address);
            assert_eq!(function, &Symbol::new(&env, "upgrade"));
        }
        _ => panic!("expected a contract invocation"),
    }
}

#[test]
fn upgrade_rejects_non_admin() {
    let (env, client, _admin) = setup();
    let fake_hash = BytesN::from_array(&env, &[1u8; 32]);

    env.mock_auths(&[]);
    let result = client.try_upgrade(&fake_hash);
    assert!(result.is_err());
}

#[test]
fn upgrade_fails_with_unuploaded_wasm_hash() {
    let (env, client, _admin) = setup();
    let unuploaded_hash = BytesN::from_array(&env, &[42u8; 32]);
    let result = client.try_upgrade(&unuploaded_hash);
    assert!(result.is_err());
}

#[test]
fn upgrade_preserves_state() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Doctors Without Borders");

    client.register(&owner, &name);
    client.approve_ngo(&owner);

    let ngo_before = client.get_ngo(&owner);
    assert!(ngo_before.verified);
    assert_eq!(ngo_before.name, name);
    assert_eq!(client.ngo_count(), 1);

    // Negative case: attempting an upgrade with an unuploaded wasm hash fails
    let unuploaded_hash = BytesN::from_array(&env, &[99u8; 32]);
    assert!(client.try_upgrade(&unuploaded_hash).is_err());

    // State reads back identically after
    let ngo_after = client.get_ngo(&owner);
    assert_eq!(ngo_before.owner, ngo_after.owner);
    assert_eq!(ngo_before.name, ngo_after.name);
    assert_eq!(ngo_before.verified, ngo_after.verified);
    assert_eq!(client.ngo_count(), 1);
}
// list_ngos pagination tests (issue #283)
// =============================================================================

#[test]
fn list_ngos_empty_registry_returns_empty_list() {
    let (env, client, _admin) = setup();
    let result = client.list_ngos(&0, &10);
    assert_eq!(result.len(), 0);
    assert_eq!(result, Vec::new(&env));
}

#[test]
fn list_ngos_one_ngo() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Only NGO"));

    let result = client.list_ngos(&0, &10);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), owner);
}

#[test]
fn list_ngos_across_page_boundaries() {
    let (env, client, _admin) = setup();
    let mut owners = soroban_sdk::vec![&env];
    for _ in 0..5 {
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "NGO");
        client.register(&owner, &name);
        owners.push_back(owner);
    }

    // Page 1: start = 0, limit = 2 -> owners [0, 1]
    let page1 = client.list_ngos(&0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), owners.get(0).unwrap());
    assert_eq!(page1.get(1).unwrap(), owners.get(1).unwrap());

    // Page 2: start = 2, limit = 2 -> owners [2, 3]
    let page2 = client.list_ngos(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), owners.get(2).unwrap());
    assert_eq!(page2.get(1).unwrap(), owners.get(3).unwrap());

    // Page 3: start = 4, limit = 2 -> owners [4]
    let page3 = client.list_ngos(&4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap(), owners.get(4).unwrap());
}

#[test]
fn list_ngos_start_beyond_end_returns_empty_list() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "NGO"));

    let result = client.list_ngos(&10, &5);
    assert_eq!(result.len(), 0);
}

#[test]
fn list_ngos_limit_larger_than_cap_is_clamped_to_max_limit() {
    let (env, client, _admin) = setup();
    // Register 55 NGOs (> MAX_LIST_LIMIT = 50)
    let mut registered = soroban_sdk::vec![&env];
    for _ in 0..55 {
        let owner = Address::generate(&env);
        client.register(&owner, &String::from_str(&env, "NGO"));
        registered.push_back(owner);
    }

    // Request limit = 100, must be clamped to 50
    let page = client.list_ngos(&0, &100);
    assert_eq!(page.len(), MAX_LIST_LIMIT);
    for i in 0..MAX_LIST_LIMIT {
        assert_eq!(page.get(i).unwrap(), registered.get(i).unwrap());
    }

    // Remaining 5 can be fetched in next page
    let remaining = client.list_ngos(&MAX_LIST_LIMIT, &50);
    assert_eq!(remaining.len(), 5);
    for i in 0..5 {
        assert_eq!(
            remaining.get(i).unwrap(),
            registered.get(MAX_LIST_LIMIT + i).unwrap()
        );
    }
}

#[test]
fn list_ngos_unregister_removes_from_middle_and_maintains_index() {
    let (env, client, _admin) = setup();
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);
    let owner_c = Address::generate(&env);

    client.register(&owner_a, &String::from_str(&env, "NGO A"));
    client.register(&owner_b, &String::from_str(&env, "NGO B"));
    client.register(&owner_c, &String::from_str(&env, "NGO C"));

    let initial = client.list_ngos(&0, &10);
    assert_eq!(initial.len(), 3);
    assert_eq!(initial.get(0).unwrap(), owner_a);
    assert_eq!(initial.get(1).unwrap(), owner_b);
    assert_eq!(initial.get(2).unwrap(), owner_c);

    // Unregister B (middle of list). Swap-and-pop moves C into position 1.
    client.unregister(&owner_b);

    let after_unregister = client.list_ngos(&0, &10);
    assert_eq!(after_unregister.len(), 2);
    assert_eq!(after_unregister.get(0).unwrap(), owner_a);
    assert_eq!(after_unregister.get(1).unwrap(), owner_c);

    // B must not be listed
    assert_eq!(client.try_get_ngo(&owner_b), Err(Ok(Error::NotRegistered)));
}

#[test]
fn list_ngos_re_registering_after_unregister_has_no_duplicates() {
    let (env, client, _admin) = setup();
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);
    let owner_c = Address::generate(&env);

    client.register(&owner_a, &String::from_str(&env, "NGO A"));
    client.register(&owner_b, &String::from_str(&env, "NGO B"));
    client.register(&owner_c, &String::from_str(&env, "NGO C"));

    // Unregister B
    client.unregister(&owner_b);

    // Re-register B
    client.register(&owner_b, &String::from_str(&env, "NGO B New"));

    let result = client.list_ngos(&0, &10);
    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap(), owner_a);
    assert_eq!(result.get(1).unwrap(), owner_c);
    assert_eq!(result.get(2).unwrap(), owner_b);
}

#[test]
fn list_ngos_unregister_last_and_only_entry() {
    let (env, client, _admin) = setup();
    let owner = Address::generate(&env);
    client.register(&owner, &String::from_str(&env, "Solo NGO"));

    assert_eq!(client.list_ngos(&0, &10).len(), 1);

    client.unregister(&owner);

    assert_eq!(client.list_ngos(&0, &10).len(), 0);
}
