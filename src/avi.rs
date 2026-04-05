/// An AVI InfoFrame.
///
/// Carries color space, colorimetry, quantization range, aspect ratio, active
/// format, VIC, and pixel repetition count. Required for the sink to display
/// the signal correctly.
///
/// Fields and encode/decode support are added in a subsequent implementation
/// phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AviInfoFrame {}
