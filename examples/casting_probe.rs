//! Synthetic AirPlay proof. Never reads a music account or opens local speakers.
//! Usage: casting_probe PORT [HOST [--silence]]; defaults to the loopback fixture.
use libresonic::playback::{AudioOutput, Cancellation, PcmBlock};
use sonicfast::{
    casting::{Kind, Settings, Shared, airplay::Sender},
    eq,
    vis::AudioTap,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let port = args.get(1).ok_or("receiver port required")?.parse()?;
    let host = args.get(2).cloned().unwrap_or_else(|| "127.0.0.1".into());
    let silent = args.get(3).is_some_and(|arg| arg == "--silence");
    let shared = Shared::default();
    shared
        .configure(Settings {
            output: Some(Kind::AirPlay),
            host,
            port,
            name: "Synthetic fixture".into(),
            allow_lan: true,
            ..Default::default()
        })
        .await?;
    tokio::task::spawn_blocking(move || -> Result<(), libresonic::playback::OutputError> {
        let mut output = Sender::new(shared, eq::shared(), AudioTap::new());
        let cancel = Cancellation::default();
        output.set_volume(u16::MAX);
        for chunk in 0..500 {
            let samples: Vec<f64> = (0..882)
                .flat_map(|n| {
                    let sample = if silent {
                        0.0
                    } else {
                        0.15 * (std::f64::consts::TAU * 660.0 * f64::from(chunk * 882 + n)
                            / 44100.0)
                            .sin()
                    };
                    [sample, sample]
                })
                .collect();
            output.write(PcmBlock::new(&samples, 44100, 2)?, &cancel)?;
        }
        output.finish(&cancel)?;
        while !output.drained()? {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cancel.cancel();
        assert_eq!(
            output.write(PcmBlock::new(&[0.1; 128], 44100, 2)?, &cancel),
            Err(libresonic::playback::OutputError::Cancelled)
        );
        output.stop();
        Ok(())
    })
    .await?
    .map_err(|e| format!("Synthetic AirPlay source failed: {e:?}"))?;
    println!("AirPlay synthetic source completed and cancelled cleanly");
    Ok(())
}
