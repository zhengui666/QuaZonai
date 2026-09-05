// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Native Rust-only Nautilus fixture. Adapted from the official v2.0.0rc4
//! engine_ema_cross example; original copyright and LGPL notice retained above.
//! This verifies execution, not target-portfolio or production acceptance.

use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_model::{
    data::{Data, QuoteTick},
    enums::{AccountType, BookType, OmsType},
    identifiers::{InstrumentId, Venue},
    instruments::{stubs::audusd_sim, Instrument, InstrumentAny},
    types::{Money, Price, Quantity},
};
use nautilus_trading::examples::strategies::EmaCross;

const VENUE: &str = "SIM";
const STARTING_BALANCE: &str = "1_000_000 USD";
const TRADE_SIZE: &str = "100000";
const EMA_FAST_PERIOD: usize = 10;
const EMA_SLOW_PERIOD: usize = 20;

fn quote(instrument_id: InstrumentId, bid: &str, ask: &str, ts: u64) -> Data {
    Data::Quote(QuoteTick::new(
        instrument_id,
        Price::from(bid),
        Price::from(ask),
        Quantity::from("100000"),
        Quantity::from("100000"),
        ts.into(),
        ts.into(),
    ))
}

fn generate_quotes(instrument_id: InstrumentId) -> Vec<Data> {
    let spread = 0.00020;
    let base_ts: u64 = 1_735_689_600_000_000_000; // 2025-01-01T00:00:00Z
    let interval: u64 = 1_000_000_000;
    let mut quotes = Vec::new();
    let mut tick: u64 = 0;

    let mut add = |mid: f64| {
        let bid = format!("{mid:.5}");
        let ask = format!("{:.5}", mid + spread);
        quotes.push(quote(instrument_id, &bid, &ask, base_ts + tick * interval));
        tick += 1;
    };

    // Flat initialization - both EMAs converge around 0.65000
    for _ in 0..25 {
        add(0.65000);
    }

    // Repeated up/down cycles to generate multiple crossovers
    let cycles = 6;
    for cycle in 0..cycles {
        let base = 0.65000 + (cycle as f64 * 0.00100);

        // Ramp up - fast EMA crosses above slow → BUY signal
        for i in 0..40 {
            add(base + (i as f64 * 0.00050));
        }

        // Ramp down - fast EMA crosses below slow → SELL signal
        for i in 0..80 {
            let peak = base + 39.0 * 0.00050;
            add(peak - (i as f64 * 0.00050));
        }
    }

    quotes
}

pub(crate) fn native_backtest() -> anyhow::Result<(usize, usize, usize)> {
    let mut engine = BacktestEngine::new(BacktestEngineConfig::default())?;

    let outcome = (|| {
        engine.add_venue(
            SimulatedVenueConfig::builder()
                .venue(Venue::from(VENUE))
                .oms_type(OmsType::Hedging)
                .account_type(AccountType::Margin)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec![Money::from(STARTING_BALANCE)])
                .build()?,
        )?;

        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        let instrument_id = instrument.id();
        engine.add_instrument(&instrument)?;

        engine.add_strategy(EmaCross::new(
            instrument_id,
            Quantity::from(TRADE_SIZE),
            EMA_FAST_PERIOD,
            EMA_SLOW_PERIOD,
        ))?;

        let quotes = generate_quotes(instrument_id);
        engine.add_data(quotes, None, true, true)?;
        engine.run(None, None, None, false)?;

        let result = engine.get_result();
        anyhow::ensure!(result.iterations == 745, "NATIVE_ITERATION_MISMATCH");
        anyhow::ensure!(
            result.total_orders > 0 && result.total_events > 0,
            "NO_NATIVE_EXECUTION_EVENTS"
        );
        Ok((result.iterations, result.total_orders, result.total_events))
    })();
    engine.dispose();
    outcome
}
