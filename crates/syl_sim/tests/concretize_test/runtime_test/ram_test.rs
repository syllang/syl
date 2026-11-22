use super::*;
#[test]
fn all_ram_ports_track_old_words_and_uninitialized_reads() {
    for top in ["SinglePortRam", "SimpleDualPortRam", "DistributedRam"] {
        let single = top == "SinglePortRam";
        let distributed = top == "DistributedRam";
        let d = design(top, RAM);
        let mut s = Simulator::new(&d);
        let mut words = [None; 16];
        let mut output = None;
        let mut step = |enabled: bool, write: bool, read: bool, wa: usize, ra: usize, data: u64| {
            if single {
                drive(&mut s, "en", enabled.into());
                drive(&mut s, "we", write.into());
                drive(&mut s, "addr", if write { wa } else { ra } as u64);
            } else {
                drive(&mut s, "we", (enabled && write).into());
                drive(&mut s, "waddr", wa as u64);
                drive(&mut s, "raddr", ra as u64);
                if !distributed {
                    drive(&mut s, "re", (enabled && read).into());
                }
            }
            drive(&mut s, "wdata", data);
            s.eval_comb().unwrap();
            let check = |s: &Simulator, v: Option<u64>| {
                assert_eq!(
                    s.read_named("rdata").unwrap(),
                    &v.map_or_else(|| Value::x(8), |n| Value::from_u64(8, n)),
                    "{top} cycle {}",
                    s.cycle()
                );
            };
            check(&s, if distributed { words[ra] } else { output });
            if enabled && (if single { !write } else { read }) {
                output = words[ra];
            }
            if enabled && write {
                words[wa] = Some(data & 255);
            }
            tick(&mut s);
            check(&s, if distributed { words[ra] } else { output });
            assert_eq!(s.memory(MemId(0), wa as u32).unwrap().as_u64(), words[wa]);
        };
        step(true, false, true, 0, 7, 0);
        for a in 0..16 {
            step(true, true, false, a, 0, (a * 17 + 5) as u64);
        }
        for a in 0..16 {
            step(true, false, true, 0, a, 0);
        }
        for a in 0..16 {
            step(false, true, true, a, 15 - a, 255);
            step(true, false, true, 0, a, 0);
        }
        for a in 0..16 {
            step(true, true, false, a, 0, (a * 29 + 11) as u64);
        }
        for a in 0..16 {
            step(true, false, true, 0, (a * 7) % 16, 0);
        }
        if !single {
            for a in 0..16 {
                step(true, true, true, a, (a + 3) % 16, (a * 13 + 7) as u64);
                step(true, true, false, a, (a + 5) % 16, (a * 19 + 3) as u64);
            }
            step(true, true, true, 6, 6, 0x92);
            step(true, false, true, 0, 6, 0);
        }
        let mut r = Random::new(0x3b02);
        for _ in 0..256 {
            step(
                !r.next().is_multiple_of(4),
                r.next() & 1 != 0,
                r.next() & 1 != 0,
                (r.next() % 16) as usize,
                (r.next() % 16) as usize,
                u64::from(r.next() as u8),
            );
        }
        for a in 0..16 {
            step(true, false, true, 0, a, 0);
        }
    }
}
