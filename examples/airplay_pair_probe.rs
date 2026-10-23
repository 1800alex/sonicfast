//! Explicit native PIN proof; credentials live only in memory for this process.
//! Usage: airplay_pair_probe discover | HOST PORT [--silence]
use libresonic::playback::{AudioOutput, Cancellation, PcmBlock};
use sonicfast::{
    casting::{
        self, Kind, Settings, Shared,
        airplay::{
            Sender,
            pairing::{Pin, Stage},
        },
    },
    eq,
    vis::AudioTap,
};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|v| v == "discover") {
        for receiver in tokio::task::spawn_blocking(casting::discover).await?? {
            println!(
                "{} · {} · {} · PIN={} · supported={}",
                receiver.name,
                receiver.kind.label(),
                receiver.address,
                receiver.pairing,
                receiver.supported
            );
        }
        return Ok(());
    }
    let host = args.first().ok_or("receiver host required")?.clone();
    let port = args.get(1).ok_or("receiver port required")?.parse()?;
    let silent = args.get(2).is_some_and(|v| v == "--silence");
    let reconnect = args.iter().any(|v| v == "--reconnect");
    let shared = Shared::default();
    let settings = Settings {
        output: Some(Kind::AirPlay),
        host,
        port,
        airplay2: true,
        allow_lan: true,
        ..Default::default()
    };
    shared.configure(settings.clone()).await?;
    shared.begin_pairing(settings, true);
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut submitted = false;
    loop {
        let status = shared.pairing_status();
        match status.stage {
            Stage::Failed => return Err(status.message.into()),
            Stage::Paired => break,
            Stage::Pin if !submitted => {
                println!("Enter the four-digit AirPlay PIN displayed on the selected receiver:");
                let pin = tokio::task::spawn_blocking(|| {
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line)?;
                    Ok::<_, std::io::Error>(Pin(line.trim().to_owned()))
                })
                .await??;
                shared.finish_pairing(status.attempt, pin);
                submitted = true;
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err("Pairing expired".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!(
        "Pairing complete; starting ten seconds of {}",
        if silent {
            "silence"
        } else {
            "quiet 660 Hz tone"
        }
    );
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut output = Sender::new(shared.clone(), eq::shared(), AudioTap::new());
        let cancel = Cancellation::default();
        output.set_volume(u16::MAX);
        for chunk in 0..500 {
            if reconnect && chunk == 250 {
                output.pause();
                output.resume().map_err(|_| shared.status())?;
            }
            let samples: Vec<f64> = (0..882)
                .flat_map(|n| {
                    let sample = if silent {
                        0.0
                    } else {
                        0.05 * (std::f64::consts::TAU * 660.0 * f64::from(chunk * 882 + n)
                            / 44100.0)
                            .sin()
                    };
                    [sample, sample]
                })
                .collect();
            output
                .write(PcmBlock::new(&samples, 44100, 2).unwrap(), &cancel)
                .map_err(|_| shared.status())?;
        }
        output.finish(&cancel).map_err(|_| shared.status())?;
        while !output.drained().map_err(|_| shared.status())? {
            std::thread::sleep(Duration::from_millis(10));
        }
        output.stop();
        Ok(())
    })
    .await??;
    println!("Authenticated stream completed and closed");
    Ok(())
}
