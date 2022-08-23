/* Copyright (c) 2020-2021 Alibaba Cloud and Intel Corporation
 *
 * SPDX-License-Identifier: Apache-2.0
 */
use foreign_types::{ForeignType, ForeignTypeRef, Opaque};
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::RawFd;
use std::os::unix::prelude::AsRawFd;
use std::ptr::NonNull;
use std::sync::Arc;
use tokio::io::DuplexStream;
use tokio::net::TcpStream;
use tokio_util::io::SyncIoBridge;

mod ffi;
use ffi::*;

pub struct RatsTlsRef(Opaque);

unsafe impl ForeignTypeRef for RatsTlsRef {
    type CType = rats_tls_handle;
}

#[derive(Clone)]
pub struct RatsTls(NonNull<rats_tls_handle>);

unsafe impl Send for RatsTlsRef {}
unsafe impl Sync for RatsTlsRef {}
unsafe impl Send for RatsTls {}
unsafe impl Sync for RatsTls {}

unsafe impl ForeignType for RatsTls {
    type CType = rats_tls_handle;
    type Ref = RatsTlsRef;

    unsafe fn from_ptr(ptr: *mut rats_tls_handle) -> RatsTls {
        RatsTls(NonNull::new_unchecked(ptr))
    }

    fn as_ptr(&self) -> *mut rats_tls_handle {
        self.0.as_ptr()
    }

    fn into_ptr(self) -> *mut rats_tls_handle {
        let inner = self.as_ptr();
        ::core::mem::forget(self);
        inner
    }
}

impl Drop for RatsTls {
    fn drop(&mut self) {
        unsafe {
            rats_tls_cleanup(self.as_ptr());
        }
    }
}

impl Deref for RatsTls {
    type Target = RatsTlsRef;

    fn deref(&self) -> &RatsTlsRef {
        unsafe { RatsTlsRef::from_ptr(self.as_ptr()) }
    }
}

impl DerefMut for RatsTls {
    fn deref_mut(&mut self) -> &mut RatsTlsRef {
        unsafe { RatsTlsRef::from_ptr_mut(self.as_ptr()) }
    }
}

impl RatsTls {
    pub fn new(
        server: bool,
        enclave_id: u64,
        tls_type: Option<&str>,
        crypto: Option<&str>,
        attester: Option<&str>,
        verifier: Option<&str>,
        mutual: bool,
    ) -> Result<RatsTls, rats_tls_err_t> {
        let mut conf: rats_tls_conf_t = Default::default();
        conf.api_version = RATS_TLS_API_VERSION_DEFAULT;
        conf.log_level = RATS_TLS_LOG_LEVEL_DEBUG;
        if let Some(tls_type) = tls_type {
            conf.tls_type[..tls_type.len()].copy_from_slice(tls_type.as_bytes());
        }
        if let Some(crypto) = crypto {
            conf.crypto_type[..crypto.len()].copy_from_slice(crypto.as_bytes());
        }
        if let Some(attester) = attester {
            conf.attester_type[..attester.len()].copy_from_slice(attester.as_bytes());
        }
        if let Some(verifier) = verifier {
            conf.verifier_type[..verifier.len()].copy_from_slice(verifier.as_bytes());
        }
        conf.cert_algo = RATS_TLS_CERT_ALGO_DEFAULT;
        conf.enclave_id = enclave_id;
        if mutual {
            conf.flags |= RATS_TLS_CONF_FLAGS_MUTUAL;
        }
        if server {
            conf.flags |= RATS_TLS_CONF_FLAGS_SERVER;
        }

        let mut handle: rats_tls_handle = unsafe { std::mem::zeroed() };
        let mut tls: *mut rats_tls_handle = &mut handle;
        let err = unsafe { rats_tls_init(&conf, &mut tls) };
        if err != RATS_TLS_ERR_NONE {
            // error!("rats_tls_init() failed");
            return Err(err);
        }

        let err = unsafe { rats_tls_set_verification_callback(&mut tls, None) };
        if err == RATS_TLS_ERR_NONE {
            Ok(unsafe { RatsTls::from_ptr(tls) })
        } else {
            Err(err)
        }
    }

    pub async fn negotiate_async(self, stream: TcpStream) -> std::io::Result<DuplexStream> {
        // Convert to std::net::TcpStream, then set socket to non-block
        let std_tcp_stream = stream.into_std().and_then(|std_tcp_stream| {
            std_tcp_stream
                .set_nonblocking(false)
                .and(Ok(std_tcp_stream))
        })?;

        let rats_tls_session = Arc::new((self, std_tcp_stream));

        {
            let rats_tls_session = rats_tls_session.clone();
            tokio::task::spawn_blocking(move || {
                rats_tls_session
                    .0
                    .negotiate(rats_tls_session.1.as_raw_fd())
                    .map_err(|err| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("rats-tls error code: {}", err),
                        )
                    })
            })
            .await??;
        }

        // TODO: Introduce async mode for librats_tls.so to replace spawn_blocking
        let (s1, s2) = tokio::io::duplex(1024);

        let (rh, wh) = tokio::io::split(s1);

        {
            let rats_tls_session = rats_tls_session.clone();
            tokio::task::spawn_blocking(move || {
                let mut rh = SyncIoBridge::new(rh);
                let mut buf = vec![0; 1024];
                'outer: while let Ok(r_len) = rh.read(&mut buf) {
                    if r_len == 0 {
                        break; // no more data to read
                    }
                    let mut w_off = 0;
                    while w_off < r_len {
                        match rats_tls_session.0.transmit(&buf[w_off..r_len]) {
                            Ok(w_len) => w_off += w_len,
                            Err(_err) => {
                                // TODO: Better way to show error message
                                // error!("Failed in rats-tls teansmit(): error {}", err);
                                break 'outer;
                            }
                        };
                    }
                }
            });
        }
        {
            tokio::task::spawn_blocking(move || {
                let mut wh = SyncIoBridge::new(wh);
                let mut buf = vec![0; 1024];
                loop {
                    match rats_tls_session.0.receive(&mut buf) {
                        Ok(r_len) => {
                            if wh.write_all(&buf[..r_len]).is_err() {
                                return;
                            }
                        }
                        Err(_err) => {
                            // TODO: Better way to show error message
                            // error!("Failed in rats-tls receive(): error {}", err);
                            return;
                        }
                    };
                }
            });
        }

        Ok(s2)
    }

    pub fn negotiate(&self, fd: RawFd) -> Result<(), rats_tls_err_t> {
        let err = unsafe { rats_tls_negotiate(self.as_ptr(), fd) };
        if err == RATS_TLS_ERR_NONE {
            Ok(())
        } else {
            Err(err)
        }
    }

    pub fn receive(&self, buf: &mut [u8]) -> Result<usize, rats_tls_err_t> {
        let mut len: size_t = buf.len() as size_t;
        let err = unsafe {
            rats_tls_receive(
                self.as_ptr(),
                buf.as_mut_ptr() as *mut ::std::os::raw::c_void,
                &mut len,
            )
        };
        if err == RATS_TLS_ERR_NONE {
            Ok(len as usize)
        } else {
            Err(err)
        }
    }

    pub fn transmit(&self, buf: &[u8]) -> Result<usize, rats_tls_err_t> {
        let mut len: size_t = buf.len() as size_t;
        let err = unsafe {
            rats_tls_transmit(
                self.as_ptr(),
                buf.as_ptr() as *const ::std::os::raw::c_void,
                &mut len,
            )
        };
        if err == RATS_TLS_ERR_NONE {
            Ok(len as usize)
        } else {
            Err(err)
        }
    }
}
