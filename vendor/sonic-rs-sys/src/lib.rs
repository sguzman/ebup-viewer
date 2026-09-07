#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[repr(C)]
pub struct sonicStreamStruct {
    _private: [u8; 0],
}

pub type sonicStream = *mut sonicStreamStruct;

unsafe extern "C" {
    pub fn sonicCreateStream(sampleRate: ::std::os::raw::c_int, numChannels: ::std::os::raw::c_int) -> sonicStream;
    pub fn sonicDestroyStream(stream: sonicStream);
    pub fn sonicWriteFloatToStream(stream: sonicStream, samples: *const f32, numSamples: ::std::os::raw::c_int) -> ::std::os::raw::c_int;
    pub fn sonicReadFloatFromStream(stream: sonicStream, samples: *mut f32, maxSamples: ::std::os::raw::c_int) -> ::std::os::raw::c_int;
    pub fn sonicFlushStream(stream: sonicStream) -> ::std::os::raw::c_int;
    pub fn sonicSamplesAvailable(stream: sonicStream) -> ::std::os::raw::c_int;
    pub fn sonicSetSpeed(stream: sonicStream, speed: f32);
    pub fn sonicSetPitch(stream: sonicStream, pitch: f32);
    pub fn sonicSetVolume(stream: sonicStream, volume: f32);
}
