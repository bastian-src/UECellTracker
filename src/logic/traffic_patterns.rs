use std::collections::VecDeque;
use anyhow::Result;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::math_util::{calculate_mean_variance, calculate_median, standardize_feature_vec};

#[derive(
    Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize, Default,
)]
pub enum RntiMatchingTrafficPatternType {
    #[default]
    A, /* t: 10 sec,  128B packets,  1ms interval =>    ?  Mbit/s */
    B, /* t: 10 sec,  128B packets,  5ms interval =>    ?  Mbit/s */
    C, /* t: 10 sec,  128B packets, 10ms interval =>    ?  Mbit/s */
    D, /* t: 10 sec,  128B packets, 15ms interval => ~ 5.8 Mbit/s */
    E, /* t: 10 sec,  128B packets, 20ms interval =>    ?  Mbit/s */
    F, /* t: 10 sec,  128B packets, 40ms interval =>    ?  Mbit/s */
    G, /* sinus       128B packets,  5ms interval =>    ?  Mbit/s */
    H, /* t: 10 sec,  256B packets,  1ms interval =>    ?  Mbit/s */
    I, /* t: 10 sec,  256B packets,  5ms interval =>    ?  Mbit/s */
    J, /* t: 10 sec,  256B packets, 10ms interval =>    ?  Mbit/s */
    K, /* t: 10 sec,  256B packets, 15ms interval => ~ 5.8 Mbit/s */
    L, /* t: 10 sec,  256B packets, 20ms interval =>    ?  Mbit/s */
    M, /* t: 10 sec,  256B packets, 40ms interval =>    ?  Mbit/s */
    N, /* t: 10 sec,  256B packets, 40ms interval | 5 sec send, 5 sec nothing */
    O,
    P, /* 5s "small" + 5s "big" */
    Q, /* test "no" traffic */
    R, /* test partial traffic */
    S, /* 0.6 MB/s and 1.6 MB/s */
    T, /* 6.1 MB/s */
    U, /* ~300 KB/s for 20 seconds */
    V, /* 12s, increasing packet size from 1B to 10000B */
    W, /* same as V, but larger packet size */
    X, /* For some reason, results only in ~130KB/s */
    Y, /* Like U, increment but more t ~ 22sec */
    Z, /* t: 24 sec, 32KB packets, 3ms interval => ? Mbit/s */
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TrafficPattern {
    pub messages: VecDeque<TrafficPatternMessage>,
    pub pattern_type: RntiMatchingTrafficPatternType,
    /* Standardization Vector: (mean, std deviation) */
    pub std_vec: Vec<(f64, f64)>,
}

/* TrafficPatternFeatures
 *
 * This struct can be attached to other structs when those shall not
 * contain the whole TrafficPattern.
 * */
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TrafficPatternFeatures {
    pub pattern_type: RntiMatchingTrafficPatternType,
    /* Standardization Vector: (mean, std deviation) */
    pub std_vec: Vec<(f64, f64)>,
    /* Standardized feature vector */
    pub std_feature_vec: Vec<f64>,
    pub total_ul_bytes: u64,
    pub nof_packets: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrafficPatternMessage {
    pub time_ms: u16,
    pub payload: Vec<u8>,
}

impl RntiMatchingTrafficPatternType {
    pub fn generate_pattern(&self) -> TrafficPattern {
        match self {
            RntiMatchingTrafficPatternType::A => pattern_a(),
            RntiMatchingTrafficPatternType::B => pattern_b(),
            RntiMatchingTrafficPatternType::C => pattern_c(),
            RntiMatchingTrafficPatternType::D => pattern_d(),
            RntiMatchingTrafficPatternType::E => pattern_e(),
            RntiMatchingTrafficPatternType::F => pattern_f(),
            RntiMatchingTrafficPatternType::G => pattern_g(),
            RntiMatchingTrafficPatternType::H => pattern_h(),
            RntiMatchingTrafficPatternType::I => pattern_i(),
            RntiMatchingTrafficPatternType::J => pattern_j(),
            RntiMatchingTrafficPatternType::K => pattern_k(),
            RntiMatchingTrafficPatternType::L => pattern_l(),
            RntiMatchingTrafficPatternType::M => pattern_m(),
            RntiMatchingTrafficPatternType::N => pattern_n(),
            RntiMatchingTrafficPatternType::O => pattern_o(),
            RntiMatchingTrafficPatternType::P => pattern_p(),
            RntiMatchingTrafficPatternType::Q => pattern_q(),
            RntiMatchingTrafficPatternType::R => pattern_r(),
            RntiMatchingTrafficPatternType::S => pattern_s(),
            RntiMatchingTrafficPatternType::T => pattern_t(),
            RntiMatchingTrafficPatternType::U => pattern_u(),
            RntiMatchingTrafficPatternType::V => pattern_v(),
            RntiMatchingTrafficPatternType::W => pattern_w(),
            RntiMatchingTrafficPatternType::X => pattern_x(),
            RntiMatchingTrafficPatternType::Y => pattern_y(),
            RntiMatchingTrafficPatternType::Z => pattern_z(),
        }
    }
}


impl TrafficPatternFeatures {
    pub fn from_traffic_pattern(pattern: &TrafficPattern) -> Result<TrafficPatternFeatures> {
        Ok(TrafficPatternFeatures {
            pattern_type: pattern.pattern_type,
            std_vec: pattern.std_vec.clone(),
            std_feature_vec: pattern.generate_standardized_feature_vec()?,
            total_ul_bytes: pattern.total_ul_bytes(),
            nof_packets: pattern.nof_packets(),
        })
    }
}

impl TrafficPattern {
    pub fn nof_packets(&self) -> u64 {
        self.messages.len() as u64
    }

    pub fn total_ul_bytes(&self) -> u64 {
        self.messages
            .iter()
            .map(|msg| msg.payload.len() as u64)
            .sum()
    }

    pub fn total_time_ms(&self) -> u64 {
        self.messages.iter().map(|msg| msg.time_ms as u64).sum()
    }

    /*
     * Feature vector, order matters:
     *
     * DCI count (occurences)
     * Total UL bytes
     * UL bytes median
     * UL bytes mean
     * UL bytes variance
     * DCI timestamp delta median
     * DCI timestamp delta mean
     * DCI timestamp delta variance
     * */
    pub fn generate_standardized_feature_vec(&self) -> Result<Vec<f64>> {
        let packet_sizes: Vec<f64> = self.messages
            .iter()
            .map(|t| t.payload.len() as f64)
            .collect::<Vec<f64>>();
        let time_deltas: Vec<f64> = self.messages
            .iter()
            .map(|m| m.time_ms as f64)
            .collect::<Vec<f64>>();

        let (ul_mean, ul_variance) = calculate_mean_variance(&packet_sizes)?;
        let ul_median = calculate_median(&packet_sizes)?;
        let (tx_mean, tx_variance) = calculate_mean_variance(&time_deltas)?;
        let tx_median = calculate_median(&time_deltas)?;

        let non_std_feature_vec: Vec<f64> = vec![
            packet_sizes.len() as f64,
            self.total_ul_bytes() as f64,
            ul_median,
            ul_mean,
            ul_variance,
            tx_median,
            tx_mean,
            tx_variance,
        ];

        Ok(standardize_feature_vec(&non_std_feature_vec, &self.std_vec))
    }
}

fn generate_incremental_pattern(
    interval_ms: u16,
    max_pow: u32,
    time_ms: u32,
    pause_time_ms: u16,
) -> VecDeque<TrafficPatternMessage> {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    let max_increment: usize = usize::pow(2, max_pow);
    for i in 0..(time_ms / interval_ms as u32) {
        let increment: usize = if i < max_pow {
            usize::pow(2, i)
        } else {
            max_increment
        };
        messages.push_back(TrafficPatternMessage {
            time_ms: interval_ms,
            payload: vec![0xA0; increment],
        })
    }
    messages.push_back(TrafficPatternMessage {
        time_ms: pause_time_ms,
        payload: vec![0xA0; max_increment],
    });
    messages
}

/* WARNING: The total time of a traffic pattern must be > 0
 *
 * After sending the pattern, the DCI messages are collected
 * for the total time * MATCHING_TRAFFIC_PATTERN_TIME_OVERLAP_FACTOR
 * */

fn pattern_a() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::A,
        messages: generate_incremental_pattern(1, 7, 10000, 1),
        std_vec: vec![
            (1303.524, 118.975),
            (5170909.524, 328405.228),
            (989.143, 104.019),
            (3984.789, 274.161),
            (31582535.518, 4674047.579),
            (5258.446, 482.658),
            (8269.488, 719.246),
            (96718304.958, 49552811.538)
        ],
    }
}

fn pattern_b() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::B,
        messages: generate_incremental_pattern(5, 7, 10000, 1),
        std_vec: vec![
            (1172.476, 84.147),
            (1070693.524, 88380.296),
            (436.667, 49.241),
            (914.404, 59.475),
            (1656085.165, 225857.600),
            (5258.113, 482.284),
            (9032.604, 617.573),
            (225072559.429, 136364731.413)
        ],
    }
}

fn pattern_c() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::C,
        messages: generate_incremental_pattern(10, 7, 10000, 1),
        /* DUMMY STD_VEC */
        std_vec: vec![
            (1686.060, 89.275),
            (806351.810, 59070.611),
            (404.381, 35.977),
            (477.924, 16.758),
            (285107.429, 95070.207),
            (4956.214, 69.591),
            (6269.322, 311.308),
            (154037780.727, 152680400.422)
        ],
    }
}

fn pattern_d() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::D,
        messages: generate_incremental_pattern(15, 7, 10000, 1),
        std_vec: vec![
            (1634.381, 96.456),
            (744945.238, 55137.483),
            (399.667, 57.232),
            (455.631, 16.472),
            (281419.406, 204011.793),
            (4959.494, 75.904),
            (6460.818, 378.365),
            (164091450.826, 130185927.368)
        ],
    }
}

fn pattern_e() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::E,
        messages: generate_incremental_pattern(20, 7, 10000, 1),
        /* DUMMY STD_VEC */
        std_vec: vec![
            (1590.107, 133.691),
            (761000.857, 130823.198),
            (492.190, 143.579),
            (476.569, 33.139),
            (266374.380, 74522.723),
            (4953.548, 212.955),
            (6651.133, 470.289),
            (169247947.552, 142828361.282)
        ],
    }
}

fn pattern_f() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::F,
        messages: generate_incremental_pattern(40, 7, 10000, 1),
        /* DUMMY STD_VEC */
        std_vec: vec![
            (1531.250, 305.115),
            (740691.524, 149809.699),
            (601.143, 139.415),
            (493.095, 77.938),
            (535847.681, 1587361.050),
            (4961.327, 232.483),
            (7939.456, 7846.388),
            (714104054.473, 3459849560.506)
        ],
    }
}

fn pattern_g() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();

    let amplitude = 128.0;
    let vertical_shift = 256.0;
    let angular_frequency = 1.5 * std::f64::consts::PI; // omega = pi for T = 3s
    let sending_interval_ms = 5;
    let pattern_interval_ms = 10000;

    for i in 0..(pattern_interval_ms / sending_interval_ms as u32) {
        let t = i as f64 * sending_interval_ms as f64 / 1000.0;
        let packet_size =
            (amplitude * (angular_frequency * t).sin() + vertical_shift).round() as usize;
        messages.push_back(TrafficPatternMessage {
            time_ms: sending_interval_ms,
            payload: vec![0xA0; packet_size],
        });
    }

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::G,
        messages,
        std_vec: vec![
            (1148.651, 98.184),
            (1933892.434, 154693.582),
            (635.470, 123.712),
            (1687.174, 100.500),
            (7489696.214, 690971.647),
            (5384.024, 603.123),
            (9239.849, 810.738),
            (226870405.729, 176491669.660)
        ],
    }
}

fn pattern_h() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::H,
        messages: generate_incremental_pattern(1, 8, 10000, 1),
        std_vec: vec![
            (1693.843, 119.172),
            (9066416.096, 644568.033),
            (1275.277, 95.283),
            (5357.688, 248.610),
            (46548500.312, 8135171.036),
            (4903.181, 228.600),
            (6337.070, 465.706),
            (51189570.636, 35430664.134)
        ],
    }
}

fn pattern_i() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::I,
        messages: generate_incremental_pattern(5, 8, 10000, 1),
        std_vec: vec![
            (1150.181, 82.893),
            (1906619.855, 139984.909),
            (614.843, 129.855),
            (1660.314, 98.195),
            (6746274.694, 684085.416),
            (5336.831, 460.689),
            (9199.727, 697.511),
            (238238507.979, 175873328.231)
        ],
    }
}

fn pattern_j() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::J,
        messages: generate_incremental_pattern(10, 8, 10000, 1),
        std_vec: vec![
            (1596.771, 86.693),
            (1067878.072, 76791.272),
            (404.145, 11.541),
            (668.671, 29.795),
            (883962.435, 140006.832),
            (4971.090, 22.084),
            (6627.635, 430.548),
            (176614657.043, 151487358.596)
        ],
    }
}

fn pattern_k() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::K,
        messages: generate_incremental_pattern(15, 8, 10000, 1),
        std_vec: vec![
            (1676.349, 75.166),
            (1023226.506, 55433.586),
            (691.759, 32.104),
            (610.347, 17.320),
            (487100.040, 74348.940),
            (4962.952, 9.933),
            (6292.111, 284.696),
            (152140483.957, 129298296.787)
        ],
    }
}

fn pattern_l() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::L,
        messages: generate_incremental_pattern(20, 8, 10000, 1),
        std_vec: vec![
            (1611.373, 81.038),
            (952066.120, 54675.723),
            (692.530, 31.421),
            (590.871, 17.377),
            (445069.843, 67743.001),
            (4970.795, 17.327),
            (6551.890, 350.497),
            (170340777.801, 137116780.191)
        ],
    }
}

fn pattern_m() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::M,
        messages: generate_incremental_pattern(40, 8, 10000, 1),
        std_vec: vec![
            (1581.639, 236.698),
            (889980.723, 360202.897),
            (693.494, 107.704),
            (553.752, 73.131),
            (359658.484, 76077.035),
            (4912.337, 336.816),
            (6711.155, 593.530),
            (160538634.769, 144251782.604)
        ],
    }
}

fn pattern_n() -> TrafficPattern {
    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::N,
        messages: generate_incremental_pattern(40, 8, 5000, 5000),
        std_vec: vec![
            (881.566, 464.619),
            (510269.494, 464966.950),
            (664.675, 159.314),
            (553.049, 87.271),
            (407529.896, 147616.223),
            (4853.566, 594.289),
            (13015.614, 4761.492),
            (5964307190.392, 6330999966.655)
        ],
    }
}

fn pattern_o() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }
    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 64],
        })
    }

    for _ in 0..50 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 20,
            payload: vec![0xA0; 512],
        })
    }

    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }
    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 64],
        })
    }

    for _ in 0..50 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 20,
            payload: vec![0xA0; 512],
        })
    }

    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }
    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 64],
        })
    }

    for _ in 0..50 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 20,
            payload: vec![0xA0; 512],
        })
    }

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::O,
        messages,
        ..Default::default()
    }
}

fn pattern_p() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 8],
        })
    }
    for _ in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 16],
        })
    }
    for _ in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }
    for _ in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }
    for _ in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 32],
        })
    }

    for _ in 0..10 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 100,
            payload: vec![0xA0; 16000],
        })
    }
    for _ in 0..10 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 100,
            payload: vec![0xA0; 16000],
        })
    }
    for _ in 0..10 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 100,
            payload: vec![0xA0; 16000],
        })
    }
    for _ in 0..10 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 100,
            payload: vec![0xA0; 16000],
        })
    }
    for _ in 0..10 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 100,
            payload: vec![0xA0; 16000],
        })
    }

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::P,
        messages,
        ..Default::default()
    }
}

fn pattern_q() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    messages.push_back(TrafficPatternMessage {
        time_ms: 100,
        payload: vec![0xA0; 16000],
    });

    messages.push_back(TrafficPatternMessage {
        time_ms: 10000,
        payload: vec![0xA0; 16000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::Q,
        messages,
        ..Default::default()
    }
}

fn pattern_r() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..5000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 16],
        })
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 5000,
        payload: vec![0xA0; 16000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::R,
        messages,
        ..Default::default()
    }
}

fn pattern_s() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 10,
            payload: vec![0xA0; 16000],
        })
    }

    for _ in 0..500 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 10,
            payload: vec![0xA0; 32000],
        })
    }

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::S,
        messages,
        ..Default::default()
    }
}

fn pattern_t() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..8000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 1024],
        })
    }
    messages.push_back(TrafficPatternMessage {
        time_ms: 2000,
        payload: vec![0xA0; 64000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::T,
        messages,
        ..Default::default()
    }
}

fn pattern_u() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..10000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 2,
            payload: vec![0xA0; 16384],
        })
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 1000,
        payload: vec![0xA0; 16384],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::U,
        messages,
        ..Default::default()
    }
}

fn pattern_v() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for i in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 10,
            payload: vec![0xA0; 200 + i],
        })
    }

    for msg in pattern_i().messages.iter() {
        messages.push_back(msg.clone());
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 2000,
        payload: vec![0xA0; 64000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::V,
        messages,
        ..Default::default()
    }
}

fn pattern_w() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for i in 0..1000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 10,
            payload: vec![0xA0; 500 + 10 * i],
        })
    }

    for msg in pattern_i().messages.iter() {
        messages.push_back(msg.clone());
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 2000,
        payload: vec![0xA0; 64000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::W,
        messages,
        ..Default::default()
    }
}

fn pattern_x() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for _ in 0..20000 {
        messages.push_back(TrafficPatternMessage {
            time_ms: 1,
            payload: vec![0xA0; 64000],
        })
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 1000,
        payload: vec![0xA0; 64000],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::X,
        messages,
        ..Default::default()
    }
}

fn pattern_y() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    for i in 0..4000 {
        let extra: usize = if i < 15 {
            usize::pow(2, i)
        } else {
            usize::pow(2, 15)
        };
        messages.push_back(TrafficPatternMessage {
            time_ms: 5,
            payload: vec![0xA0; 500 + extra],
        })
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 2000,
        payload: vec![0xA0; 16384],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::Y,
        messages,
        ..Default::default()
    }
}

fn pattern_z() -> TrafficPattern {
    let mut messages: VecDeque<TrafficPatternMessage> = VecDeque::<TrafficPatternMessage>::new();
    /* to be determined */
    let max_pow = 15;
    let max_increment = usize::pow(2, max_pow);
    for i in 0..6000 {
        let increment: usize = if i < max_pow {
            usize::pow(2, i)
        } else {
            max_increment
        };
        messages.push_back(TrafficPatternMessage {
            time_ms: 3,
            payload: vec![0xA0; 500 + increment],
        })
    }

    messages.push_back(TrafficPatternMessage {
        time_ms: 4000,
        payload: vec![0xA0; max_increment],
    });

    TrafficPattern {
        pattern_type: RntiMatchingTrafficPatternType::Z,
        messages,
        ..Default::default()
    }
}
