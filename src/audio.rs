/// An Audio InfoFrame.
///
/// Carries channel count, coding type, sample frequency, sample size, channel
/// allocation, LFE playback level, and downmix inhibit. Required for the sink
/// to configure its audio decoder and speaker mapping correctly.
///
/// Fields and encode/decode support are added in a subsequent implementation
/// phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AudioInfoFrame {}
