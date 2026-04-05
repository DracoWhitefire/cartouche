/// Audio coding type (CT field, PB1 bits 6–3).
///
/// Identifies the audio format being transmitted. `ReferToStream` defers to
/// the audio bitstream's own header; this is the common case for compressed
/// formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AudioCodingType {
    /// Coding type is indicated in the audio stream header (CT = 0).
    ReferToStream,
    /// Linear PCM (CT = 1).
    Lpcm,
    /// Dolby AC-3 (CT = 2).
    Ac3,
    /// MPEG-1 layers 1 & 2 (CT = 3).
    Mpeg1,
    /// MPEG-1 layer 3 / MP3 (CT = 4).
    Mp3,
    /// MPEG-2 multi-channel (CT = 5).
    Mpeg2Multichannel,
    /// AAC LC (CT = 6).
    AacLc,
    /// DTS (CT = 7).
    Dts,
    /// ATRAC (CT = 8).
    Atrac,
    /// One Bit Audio / DSD (CT = 9).
    OneBitAudio,
    /// Enhanced AC-3 / Dolby Digital Plus (CT = 10).
    EnhancedAc3,
    /// DTS-HD (CT = 11).
    DtsHd,
    /// MLP / Dolby TrueHD (CT = 12).
    MlpTrueHd,
    /// DST (CT = 13).
    Dst,
    /// WMA Pro (CT = 14).
    WmaPro,
    /// Coding type is in the CXT extension field (CT = 15).
    Extension,
}

/// Audio channel count (CC field, PB1 bits 2–0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChannelCount {
    /// Channel count is indicated in the audio stream header (CC = 0).
    ReferToStream,
    /// 1 through 8 channels (CC = 1–7, representing CC+1 channels).
    ///
    /// The stored value is the number of channels (1–8).
    Count(u8),
}

/// Audio sample frequency (SF field, PB2 bits 4–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleFrequency {
    /// Sample frequency is indicated in the audio stream header (SF = 0).
    ReferToStream,
    /// 32 kHz (SF = 1).
    Hz32000,
    /// 44.1 kHz (SF = 2).
    Hz44100,
    /// 48 kHz (SF = 3).
    Hz48000,
    /// 88.2 kHz (SF = 4).
    Hz88200,
    /// 96 kHz (SF = 5).
    Hz96000,
    /// 176.4 kHz (SF = 6).
    Hz176400,
    /// 192 kHz (SF = 7).
    Hz192000,
}

/// Audio sample size (SS field, PB2 bits 1–0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleSize {
    /// Sample size is indicated in the audio stream header (SS = 0).
    ReferToStream,
    /// 16-bit samples (SS = 1).
    Bits16,
    /// 20-bit samples (SS = 2).
    Bits20,
    /// 24-bit samples (SS = 3).
    Bits24,
}

/// LFE channel playback level relative to the other channels (LSV field,
/// PB5 bits 6–3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LfePlaybackLevel {
    /// No LFE playback level information (LSV = 0).
    NoInfo,
    /// LFE is played back at +10 dB relative to the other channels (LSV = 1).
    Plus10Db,
    /// LFE is played back at 0 dB relative to the other channels (LSV = 2).
    Ref0Db,
}

/// An Audio InfoFrame (CEA-861, type code 0x84).
///
/// Carries the audio format metadata required by the sink to configure its
/// audio decoder and speaker mapping. Transmitted alongside the audio stream.
///
/// The `coding_ext` field is only meaningful when `coding_type` is
/// [`AudioCodingType::Extension`]; in all other cases it should be `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AudioInfoFrame {
    /// Audio coding type (CT, PB1 bits 6–3).
    pub coding_type: AudioCodingType,
    /// Number of audio channels (CC, PB1 bits 2–0).
    pub channel_count: ChannelCount,
    /// Audio sample frequency (SF, PB2 bits 4–2).
    pub sample_freq: SampleFrequency,
    /// Audio sample size (SS, PB2 bits 1–0).
    pub sample_size: SampleSize,
    /// Audio coding extension type (CXT, PB3 bits 4–0).
    ///
    /// Only meaningful when `coding_type` is [`AudioCodingType::Extension`].
    pub coding_ext: u8,
    /// Channel/speaker allocation code (CA, PB4).
    ///
    /// Selects the speaker layout. See CEA-861 Table 20 for defined values.
    /// `0x00` is stereo (FL/FR); `0x01` adds LFE; `0x02` adds FC; etc.
    pub channel_allocation: u8,
    /// LFE playback level relative to the other channels (LSV, PB5 bits 6–3).
    pub lfe_playback_level: LfePlaybackLevel,
    /// Prohibit downmixing by the sink (DM_INH, PB5 bit 7).
    ///
    /// When `true`, the sink must not produce a downmixed stereo output from
    /// the multi-channel audio stream.
    pub downmix_inhibit: bool,
}
