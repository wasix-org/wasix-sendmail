use std::env;
use std::io::{stderr, stdin, stdout};
use wasix_sendmail::run_sendmail;

fn main() {
    let args: Vec<_> = env::args().collect();
    let envs: Vec<_> = env::vars().collect();
    let exit_code = run_sendmail(&mut stdin(), &mut stdout(), &mut stderr(), &args, &envs);
    std::process::exit(exit_code);
}
