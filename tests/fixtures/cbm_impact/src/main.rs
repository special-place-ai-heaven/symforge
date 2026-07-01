mod a;
mod b;
mod c;

use cbm_impact_fixture::core;

fn main() {
    let _ = (a::call_a(), b::call_b(), c::call_c(), core());
}
