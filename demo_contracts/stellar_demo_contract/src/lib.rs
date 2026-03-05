#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Env};

#[contract]
pub struct StellarDemoContract;

#[contractimpl]
impl StellarDemoContract {
    pub fn increment(env: Env) -> u32 {
        let key = symbol_short!("count");
        let next = env
            .storage()
            .persistent()
            .get::<_, u32>(&key)
            .unwrap_or(0)
            .saturating_add(1);
        env.storage().persistent().set(&key, &next);
        next
    }

    pub fn current(env: Env) -> u32 {
        let key = symbol_short!("count");
        env.storage().persistent().get::<_, u32>(&key).unwrap_or(0)
    }
}
