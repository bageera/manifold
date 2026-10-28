// scratch: emit each model's payload hex to stdout
use mnf_mans::mnf10::*;
fn main() {
    let p1 = payload_const([7, 7, 7]);
    println!("{}", hex(&p1));
    // div-1D for the 96-wide fixture: D = 95 (index 3 = w-1); endpoints
    // 20->230, 230->20, 60->200 → N = total rise per full span: 210/-210/140,
    // T = a0*D.
    let p2 = payload_div1d([(210, 20 * 95), (-210, 230 * 95), (140, 60 * 95)], 3);
    println!("{}", hex(&p2));
    let p3 = payload_rec1([(30u8, 7i16), (200, 7), (90, 7)], 0);
    println!("{}", hex(&p3));
    let p4 = payload_bilinear(
        [[15, 200, 70], [225, 30, 180], [90, 90, 20], [245, 240, 240]],
        false,
    );
    println!("{}", hex(&p4));
    let p5 = payload_bilinear(
        [[15, 200, 70], [225, 30, 180], [90, 90, 20], [245, 240, 240]],
        true,
    );
    println!("{}", hex(&p5));
    // piecewise: [0,48) 20->130ish per channel, [48,96) 130->230
    let segs = vec![
        PiecewiseSeg {
            x_start: 0,
            per_channel: [(110, 20 * 47, 3), (110, 10 * 47, 3), (90, 60 * 47, 3)],
        },
        PiecewiseSeg {
            x_start: 48,
            per_channel: [(100, 130 * 47, 3), (115, 120 * 47, 3), (50, 150 * 47, 3)],
        },
    ];
    let p6 = payload_piecewise(&segs).unwrap();
    println!("{}", hex(&p6));
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
