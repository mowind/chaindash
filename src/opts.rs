use clap::Clap;
use num_rational::Ratio;

#[derive(Clap)]
#[clap(version = "1.0", author = "mowind <wjinwen.1988@gmail.com>")]
pub struct Opts {
    /// The platon connection endpoints, separated by `,`.
    #[clap(long, default_value = "http://127.0.0.1:6789")]
    pub url: String,

    /// Render interval
    #[clap(long, default_value = "1")]
    pub interval: Ratio<u64>,
}
