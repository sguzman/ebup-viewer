#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub type espeak_AUDIO_OUTPUT = i32;
pub type espeak_ERROR = i32;

pub const espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL: espeak_AUDIO_OUTPUT = 1;
pub const espeak_ERROR_EE_OK: espeak_ERROR = 0;
pub const espeakINITIALIZE_PHONEME_IPA: u32 = 0x0002;
pub const espeakINITIALIZE_DONT_EXIT: i32 = 0x8000;
pub const espeakCHARS_UTF8: i32 = 1;

unsafe extern "C" {
    pub fn espeak_Initialize(
        output: espeak_AUDIO_OUTPUT,
        buflength: ::std::os::raw::c_int,
        path: *const ::std::os::raw::c_char,
        options: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
    pub fn espeak_SetVoiceByName(name: *const ::std::os::raw::c_char) -> espeak_ERROR;
    pub fn espeak_TextToPhonemes(
        textptr: *mut *const ::std::os::raw::c_void,
        textmode: ::std::os::raw::c_int,
        phonememode: ::std::os::raw::c_int,
    ) -> *const ::std::os::raw::c_char;
}
