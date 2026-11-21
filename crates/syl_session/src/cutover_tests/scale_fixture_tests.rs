//! Deterministic four-layer hardware DAG shared by session and emit measurements.
//! Fixture creation and edits happen outside the measured intervals.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct ScaleFixture {
    pub(crate) root: PathBuf,
    pub(crate) roots: Vec<PathBuf>,
}

impl ScaleFixture {
    pub(crate) fn new(file_count: usize) -> Self {
        assert!(file_count >= 10 && file_count.is_multiple_of(10));
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "syl_scale_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("unique scale fixture");
        let group_count = file_count / 10;
        let leaf_count = group_count * 4;
        let roots = (0..group_count)
            .map(|i| root.join(format!("pkg_top/top_{i}.syl")))
            .collect();
        let fixture = Self { root, roots };
        for layer in ["pkg_defs", "pkg_leaf", "pkg_mid", "pkg_top"] {
            fs::create_dir(fixture.root.join(layer)).unwrap();
        }
        for i in 0..group_count {
            fixture.write(
                &format!("pkg_defs/defs_{i}.syl"),
                &format!(
                    "const W_{i}: nat = 8\nconst EXTRA_{i}: nat = 1\n\
                     enum Mode_{i} {{ Idle, Active }}\n\
                     bundle Payload_{i} {{ data: UInt<W_{i}> }}\n\
                     fn width_{i}() -> nat {{ return W_{i} + EXTRA_{i} }}\n\
                     map pass_{i}(x: UInt<W_{i}>) -> UInt<W_{i}> = x\n"
                ),
            );
        }
        for i in 0..leaf_count {
            let d = i % group_count;
            fixture.write(
                &format!("pkg_leaf/leaf_{i}.syl"),
                &format!(
                    "use pkg_defs.defs_{d}.W_{d}\n\
                     use pkg_defs.defs_{d}.Payload_{d}\n\
                     use pkg_defs.defs_{d}.pass_{d}\n\
                     cell Comb_{i}(x: in Bit) -> y: Bit {{\n\
                         signal word: UInt<W_{d}> := 0\n\
                         signal packet: Payload_{d} := Payload_{d} {{ data: word }}\n\
                         y := x xor (pass_{d}(packet.data) eq 0)\n\
                     }}\n\
                     cell Seq_{i}<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, x: in Bit, y: out Bit) {{\n\
                         reg held: Bit reset(rst, 0)\n\
                         next held := x\n\
                         y := held\n\
                     }}\n"
                ),
            );
            let next = (i + 1) % leaf_count;
            fixture.write(
                &format!("pkg_mid/mid_{i}.syl"),
                &format!(
                    "use pkg_leaf.leaf_{i}.Comb_{i}\n\
                     use pkg_leaf.leaf_{next}.Seq_{next}\n\
                     cell Mid_{i}<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, x: in Bit) -> y: Bit {{\n\
                         signal comb: Bit := place Comb_{i}(x: x)\n\
                         let seq = inplace Seq_{next}<D>(clk: clk, rst: rst, x: comb, y: y)\n\
                     }}\n"
                ),
            );
        }
        for i in 0..group_count {
            let mut source = String::new();
            for j in i * 4..i * 4 + 4 {
                source.push_str(&format!("use pkg_mid.mid_{j}.Mid_{j}\n"));
            }
            source.push_str(&format!(
                "cell Top_{i}<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, x: in Bit) -> y: Bit {{\n"
            ));
            for k in 0..4 {
                let j = i * 4 + k;
                let input = if k == 0 {
                    "x".to_string()
                } else {
                    format!("s{}", k - 1)
                };
                source.push_str(&format!(
                    "signal s{k}: Bit := place Mid_{j}<D>(clk: clk, rst: rst, x: {input})\n"
                ));
            }
            source.push_str("y := s3\n}\n");
            fixture.write(&format!("pkg_top/top_{i}.syl"), &source);
        }
        fixture
    }

    fn write(&self, relative: &str, source: &str) {
        fs::write(self.root.join(relative), source).unwrap();
    }
}

impl Drop for ScaleFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
