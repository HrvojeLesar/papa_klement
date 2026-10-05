use std::{
    io::Result as IoResult,
    process::{Child, Command, Stdio},
};

use serenity::async_trait;
use songbird::input::{
    core::io::{MediaSource, ReadOnlySource},
    AudioStream, AudioStreamError, AuxMetadata, ChildContainer, Compose, Input, RawAdapter,
};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;

// When YouTube offers no audio-only format (e.g. only HLS when using cookies),
// cap the muxed video so we don't pull 1080p just for its audio.
pub(crate) const YTDL_FORMAT: &str = "webm[abr>0]/bestaudio/best[height<=480]/best";

enum Source {
    File(String),
    YtDlp { url: String, args: Vec<String> },
}

/// Lazy input that decodes anything ffmpeg understands into raw PCM for songbird.
///
/// Symphonia can't demux MPEG-TS (YouTube HLS) or MP4, which is all YouTube
/// sometimes offers, so everything is piped through ffmpeg instead.
pub(crate) struct FfmpegInput {
    source: Source,
    metadata: AuxMetadata,
}

impl FfmpegInput {
    pub(crate) fn file(path: String, metadata: AuxMetadata) -> Self {
        Self {
            source: Source::File(path),
            metadata,
        }
    }

    pub(crate) fn ytdl(url: String, ytdl_args: Vec<String>, metadata: AuxMetadata) -> Self {
        Self {
            source: Source::YtDlp {
                url,
                args: ytdl_args,
            },
            metadata,
        }
    }

    fn spawn(&self) -> IoResult<Vec<Child>> {
        match &self.source {
            Source::File(path) => {
                let ffmpeg = ffmpeg_command(path).stdin(Stdio::null()).spawn()?;
                Ok(vec![ffmpeg])
            }
            Source::YtDlp { url, args } => {
                let mut ytdl = Command::new("yt-dlp")
                    .args(args)
                    .args([
                        "-q",
                        "--no-playlist",
                        "-f",
                        YTDL_FORMAT,
                        "-o",
                        "-",
                        "--",
                        url,
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .spawn()?;
                let ytdl_stdout = ytdl.stdout.take().expect("yt-dlp stdout is piped");
                match ffmpeg_command("pipe:0").stdin(ytdl_stdout).spawn() {
                    Ok(ffmpeg) => Ok(vec![ytdl, ffmpeg]),
                    Err(e) => {
                        let _ = ytdl.kill();
                        let _ = ytdl.wait();
                        Err(e)
                    }
                }
            }
        }
    }
}

fn ffmpeg_command(input: &str) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-loglevel", "error", "-i", input, "-vn"])
        .args(["-f", "f32le", "-ac", &CHANNELS.to_string()])
        .args(["-ar", &SAMPLE_RATE.to_string(), "pipe:1"])
        .stdout(Stdio::piped());
    command
}

#[async_trait]
impl Compose for FfmpegInput {
    fn create(&mut self) -> Result<AudioStream<Box<dyn MediaSource>>, AudioStreamError> {
        let children = self
            .spawn()
            .map_err(|e| AudioStreamError::Fail(Box::new(e)))?;
        let pcm = ReadOnlySource::new(ChildContainer::from(children));
        Ok(AudioStream {
            input: Box::new(RawAdapter::new(pcm, SAMPLE_RATE, CHANNELS)),
        })
    }

    async fn create_async(
        &mut self,
    ) -> Result<AudioStream<Box<dyn MediaSource>>, AudioStreamError> {
        Err(AudioStreamError::Unsupported)
    }

    fn should_create_async(&self) -> bool {
        false
    }

    async fn aux_metadata(&mut self) -> Result<AuxMetadata, AudioStreamError> {
        Ok(self.metadata.clone())
    }
}

impl From<FfmpegInput> for Input {
    fn from(val: FfmpegInput) -> Self {
        Input::Lazy(Box::new(val))
    }
}
