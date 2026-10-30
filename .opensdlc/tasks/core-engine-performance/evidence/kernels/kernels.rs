//! Synthetic structural microbenchmark, NOT production engine or end-to-end RT.
//! Mirrors first-party loop shapes at d7da6bff197bcbf272ba998ae0a15b2c1b25071c.
//! Target DecimalValue/Bar are projected onto Strings/u64; clone timings cannot
//! establish real NativeTargetPointV1 cost. No third-party code is copied.
use std::{collections::BTreeMap, hint::black_box, time::Instant};

#[derive(Clone)]
struct Target { instrument: String, weight: String }
#[derive(Clone)]
struct Point { asof: u64, targets: Vec<Target>, cash: String }
#[derive(Clone, Copy)]
struct Bar { instrument: usize, event: u64, init: u64 }
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct Counts { clones: u64, cloned_targets: u64, coherence_visits: u64, max_visits: u64, compares: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome { consumed: usize, digest: u64, application_events: Vec<u64> }

fn observe(point: Point, event: u64, out: &mut Outcome) {
    let point = black_box(point);
    out.consumed += 1;
    out.application_events.push(event);
    out.digest = out.digest.wrapping_add(event).wrapping_add(point.cash.len() as u64);
    for target in &point.targets {
        out.digest = out.digest.wrapping_add(target.instrument.len() as u64)
            .wrapping_add(target.weight.len() as u64);
    }
}

// mode=0: original clone-before-guard loop; mode=1: defer clone only;
// mode=2: defer clone plus exact timestamp-frequency coherence index.
#[inline(never)]
fn replay<const COUNT: bool>(bars: &[Bar], points: &[Point], assets: usize, mode: u8) -> (Outcome, Counts) {
    let mut latest = BTreeMap::<usize, Bar>::new();
    let mut frequencies = BTreeMap::<u64, usize>::new();
    let mut counts = Counts::default();
    let mut out = Outcome { consumed: 0, digest: 0, application_events: Vec::new() };
    for bar in bars {
        let prior = latest.insert(bar.instrument, *bar);
        if mode == 2 {
            if let Some(prior) = prior {
                let remaining = frequencies.get_mut(&prior.event).unwrap();
                *remaining -= 1;
                if *remaining == 0 { frequencies.remove(&prior.event); }
            }
            *frequencies.entry(bar.event).or_insert(0) += 1;
        }
        let Some(source) = points.get(out.consumed) else { continue; };
        // black_box prevents eliding the work merely because early-exit fields
        // do not consume the owned point. Production crate boundaries may differ.
        let early = if mode == 0 {
            if COUNT { counts.clones += 1; counts.cloned_targets += source.targets.len() as u64; }
            Some(black_box(source.clone()))
        } else { None };
        let coherent = latest.len() == assets && if mode == 2 {
            frequencies.len() == 1
        } else {
            !latest.values().any(|v| {
                if COUNT { counts.coherence_visits += 1; }
                v.event != bar.event
            })
        };
        if !coherent { continue; }
        let now = latest.values().map(|v| {
            if COUNT { counts.max_visits += 1; }
            v.init
        }).max().unwrap();
        if now < source.asof { continue; }
        let owned = match early {
            Some(p) => p,
            None => {
                if COUNT { counts.clones += 1; counts.cloned_targets += source.targets.len() as u64; }
                source.clone()
            }
        };
        observe(owned, now, &mut out);
    }
    (out, counts)
}

struct Forecast { instrument: String, event: u64, value: f64 }
#[inline(never)]
fn forecast<const COUNT: bool>(points: &[Forecast], ids: &[String], rows: usize, mode: u8) -> (Vec<(u64,u64)>, Counts) {
    let mut counts = Counts::default();
    let mut values = Vec::with_capacity(ids.len());
    if mode == 0 {
        for id in ids {
            let point = points.iter().rev().find(|p| {
                if COUNT { counts.compares += 1; }
                &p.instrument == id
            }).unwrap();
            values.push((black_box(point.value).to_bits(),point.event));
        }
    } else if mode == 1 {
        // Includes construction cost. Valid for arbitrary point ordering.
        let mut tails = BTreeMap::<&str, &Forecast>::new();
        for point in points { tails.insert(&point.instrument, point); }
        for id in ids {
            let point = tails[id.as_str()];
            values.push((black_box(point.value).to_bits(),point.event));
        }
    } else {
        // Valid only under the prepare() equal-rows/asset-order contract. In a
        // production refactor capture each series' tail at the producer instead.
        for (i, id) in ids.iter().enumerate() {
            let point = &points[(i + 1) * rows - 1];
            assert_eq!(&point.instrument, id);
            values.push((black_box(point.value).to_bits(),point.event));
        }
    }
    (values, counts)
}

fn quantile(values: &[f64], q: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() as f64 * q).ceil() as usize).max(1) - 1]
}
fn summarize(kernel: &str, assets: usize, rows: usize, names: &[&str], times: &[Vec<f64>], counts: &[Counts]) {
    for ((name, values), count) in names.iter().zip(times).zip(counts) {
        println!("SUMMARY,{kernel},{assets},{rows},{name},{:.6},{:.6},{},{},{},{},{}",
            quantile(values,0.5),quantile(values,0.95),count.clones,count.cloned_targets,
            count.coherence_visits,count.max_visits,count.compares);
    }
}

fn main() {
    const WARMUP: usize = 5;
    const RUNS: usize = 30;
    println!("META,synthetic_structural_microbenchmark,source_revision,d7da6bff197bcbf272ba998ae0a15b2c1b25071c,warmup,{WARMUP},samples,{RUNS},units,ms");
    // Sparse target interval: 4 target applications / 256 complete cross-sections.
    for assets in [8,32,128,256] {
        let rows = 256;
        let bars: Vec<_> = (1..=rows).flat_map(|t| (0..assets).map(move |i| Bar {
            instrument:i,event:t as u64,init:t as u64,
        })).collect();
        let points: Vec<_> = (1..=4).map(|i| Point {
            asof:i*64,
            targets:(0..assets).map(|j| Target {
                instrument:format!("ASSET{j:04}.SIM"),weight:"0.003906250000000000".to_owned(),
            }).collect(),cash:"0.000000000000000000".to_owned(),
        }).collect();
        let expected = replay::<true>(&bars,&points,assets,0).0;
        let names = ["clone_before_guard","defer_clone","defer_clone_timestamp_index"];
        let counts: Vec<_> = (0..3).map(|mode| {
            let (actual,c)=replay::<true>(&bars,&points,assets,mode);
            assert_eq!(actual,expected); c
        }).collect();
        for _ in 0..WARMUP { for mode in 0..3 { assert_eq!(replay::<false>(&bars,&points,assets,mode).0,expected); } }
        let mut times=vec![Vec::new(),Vec::new(),Vec::new()];
        for sample in 0..RUNS {
            let order=if sample%2==0 {[0,1,2]} else {[2,1,0]};
            for mode in order {
                let start=Instant::now();
                let actual=black_box(replay::<false>(black_box(&bars),black_box(&points),assets,mode));
                let ms=start.elapsed().as_secs_f64()*1000.;
                assert_eq!(actual.0,expected);
                times[mode as usize].push(ms);
                println!("SAMPLE,replay,{assets},{rows},{},{sample},{ms:.6}",names[mode as usize]);
            }
        }
        summarize("replay",assets,rows,&names,&times,&counts);
    }
    for (assets,rows) in [(8,1024),(32,1024),(128,1024),(128,4096)] {
        let ids: Vec<_>=(0..assets).map(|i|format!("ASSET{i:04}.SIM")).collect();
        let points:Vec<_>=ids.iter().enumerate().flat_map(|(i,id)| (0..rows).map(move |t|Forecast {
            instrument:id.clone(),event:t as u64,value:(i*rows+t) as f64/10_000.,
        })).collect();
        let names=["repeated_reverse_find","build_tail_map","known_block_tail_offsets"];
        let expected=forecast::<true>(&points,&ids,rows,0).0;
        let counts:Vec<_>=(0..3).map(|mode| {
            let (actual,c)=forecast::<true>(&points,&ids,rows,mode);
            assert_eq!(actual,expected);c
        }).collect();
        for _ in 0..WARMUP { for mode in 0..3 { assert_eq!(forecast::<false>(&points,&ids,rows,mode).0,expected); } }
        let mut times=vec![Vec::new(),Vec::new(),Vec::new()];
        for sample in 0..RUNS {
            let order=if sample%2==0 {[0,1,2]} else {[2,1,0]};
            for mode in order {
                let start=Instant::now();
                let actual=black_box(forecast::<false>(black_box(&points),black_box(&ids),rows,mode));
                let ms=start.elapsed().as_secs_f64()*1000.;
                assert_eq!(actual.0,expected);
                times[mode as usize].push(ms);
                println!("SAMPLE,forecast,{assets},{rows},{},{sample},{ms:.6}",names[mode as usize]);
            }
        }
        summarize("forecast",assets,rows,&names,&times,&counts);
    }
    // Non-monotone event times / partial cross-sections: the frequency index is
    // semantically equivalent to scanning latest, without assuming event order.
    let points=vec![Point{asof:0,targets:vec![],cash:String::new()};64];
    let mut seed=0x123456789abcdefu64;
    let bars:Vec<_>=(0..4096).map(|i|{
        seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;
        Bar{instrument:(seed%3) as usize,event:(seed>>8)%5,init:i}
    }).collect();
    let expected=replay::<false>(&bars,&points,3,0).0;
    assert_eq!(replay::<false>(&bars,&points,3,1).0,expected);
    assert_eq!(replay::<false>(&bars,&points,3,2).0,expected);
    println!("CHECK,all_variants_equal_on_all_workloads_and_nonmonotone_trace,PASS");
}
