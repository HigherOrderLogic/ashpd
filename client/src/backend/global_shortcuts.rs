use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use zbus::{message::Header, object_server::SignalEmitter};

use crate::{
    MaybeAppID, PortalError, WindowIdentifierType,
    backend::{
        IfaceInner, Result,
        request::{Request, RequestImpl},
        session::{CreateSessionResponse, Session, SessionImpl, SessionManager},
    },
    desktop::{
        CreateSessionOptions, HandleToken,
        global_shortcuts::{
            BindShortcuts, BindShortcutsOptions, ConfigureShortcutsOptions, ListShortcuts,
            NewShortcut, Shortcut,
        },
        request::Response,
    },
    proxy::DESKTOP_PATH,
    wire::{ObjectPath, Optional, OwnedObjectPath, OwnedVariant},
};

/// Emits the `org.freedesktop.impl.portal.GlobalShortcuts` signals.
#[async_trait]
pub trait GlobalShortcutsSignalEmitter: Send + Sync {
    /// Emitted when a shortcut becomes active.
    #[doc(alias = "Activated")]
    async fn emit_activated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()>;

    /// Emitted when a shortcut is not active anymore.
    #[doc(alias = "Deactivated")]
    async fn emit_deactivated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()>;

    /// Emitted when the information associated with some shortcuts changed.
    #[doc(alias = "ShortcutsChanged")]
    async fn emit_shortcuts_changed(
        &self,
        session_token: HandleToken,
        shortcuts: Vec<Shortcut>,
    ) -> zbus::Result<()>;
}

#[async_trait]
pub trait GlobalShortcutsImpl: RequestImpl + SessionImpl {
    #[doc(alias = "CreateSession")]
    async fn create_session(
        &self,
        token: HandleToken,
        session_token: HandleToken,
        app_id: Option<MaybeAppID>,
        options: CreateSessionOptions,
    ) -> Result<CreateSessionResponse>;

    #[doc(alias = "BindShortcuts")]
    async fn bind_shortcuts(
        &self,
        token: HandleToken,
        session_token: HandleToken,
        shortcuts: Vec<NewShortcut>,
        window_identifier: Option<WindowIdentifierType>,
        options: BindShortcutsOptions,
    ) -> Result<BindShortcuts>;

    #[doc(alias = "ListShortcuts")]
    async fn list_shortcuts(
        &self,
        token: HandleToken,
        session_token: HandleToken,
    ) -> Result<ListShortcuts>;

    #[doc(alias = "ConfigureShortcuts")]
    async fn configure_shortcuts(
        &self,
        session_token: HandleToken,
        window_identifier: Option<WindowIdentifierType>,
        options: ConfigureShortcutsOptions,
    ) -> Result<()>;

    /// Set the signal emitter, allowing to notify of shortcut changes.
    fn set_signal_emitter(&mut self, signal_emitter: Arc<dyn GlobalShortcutsSignalEmitter>);
}

pub(crate) struct GlobalShortcutsInterface {
    imp: Arc<dyn GlobalShortcutsImpl>,
    inner: IfaceInner,
    sessions: Arc<Mutex<SessionManager>>,
}

impl GlobalShortcutsInterface {
    pub fn new(
        imp: Arc<dyn GlobalShortcutsImpl>,
        inner: IfaceInner,
        sessions: Arc<Mutex<SessionManager>>,
    ) -> Self {
        Self {
            imp,
            inner,
            sessions,
        }
    }

    fn session_path(&self, token: &HandleToken) -> zbus::Result<OwnedObjectPath> {
        self.sessions
            .lock()
            .unwrap()
            .try_contains(token)
            .map(|session| session.path().clone())
            .map_err(|error| zbus::Error::Failure(error.to_string()))
    }

    pub async fn activated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let object_server = self.inner.cnx.object_server();
        let iface_ref = object_server.interface::<_, Self>(DESKTOP_PATH).await?;
        Self::activated_signal(
            iface_ref.signal_emitter(),
            path.as_ref(),
            shortcut_id,
            timestamp,
            options,
        )
        .await
    }

    pub async fn deactivated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let object_server = self.inner.cnx.object_server();
        let iface_ref = object_server.interface::<_, Self>(DESKTOP_PATH).await?;
        Self::deactivated_signal(
            iface_ref.signal_emitter(),
            path.as_ref(),
            shortcut_id,
            timestamp,
            options,
        )
        .await
    }

    pub async fn shortcuts_changed(
        &self,
        session_token: HandleToken,
        shortcuts: Vec<Shortcut>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let object_server = self.inner.cnx.object_server();
        let iface_ref = object_server.interface::<_, Self>(DESKTOP_PATH).await?;
        Self::shortcuts_changed_signal(iface_ref.signal_emitter(), path.as_ref(), shortcuts).await
    }
}

#[async_trait]
impl GlobalShortcutsSignalEmitter for GlobalShortcutsInterface {
    async fn emit_activated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        self.activated(session_token, shortcut_id, timestamp, options)
            .await
    }

    async fn emit_deactivated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        self.deactivated(session_token, shortcut_id, timestamp, options)
            .await
    }

    async fn emit_shortcuts_changed(
        &self,
        session_token: HandleToken,
        shortcuts: Vec<Shortcut>,
    ) -> zbus::Result<()> {
        self.shortcuts_changed(session_token, shortcuts).await
    }
}

#[derive(Clone)]
pub(crate) struct GlobalShortcutsEmitter {
    inner: IfaceInner,
    sessions: Arc<Mutex<SessionManager>>,
}

impl GlobalShortcutsEmitter {
    pub(crate) fn new(inner: IfaceInner, sessions: Arc<Mutex<SessionManager>>) -> Self {
        Self { inner, sessions }
    }

    fn session_path(&self, token: &HandleToken) -> zbus::Result<OwnedObjectPath> {
        self.sessions
            .lock()
            .unwrap()
            .try_contains(token)
            .map(|session| session.path().clone())
            .map_err(|error| zbus::Error::Failure(error.to_string()))
    }

    fn emitter(&self) -> zbus::Result<SignalEmitter<'static>> {
        SignalEmitter::new(&self.inner.cnx, DESKTOP_PATH)
    }
}

#[async_trait]
impl GlobalShortcutsSignalEmitter for GlobalShortcutsEmitter {
    async fn emit_activated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let emitter = self.emitter()?;
        GlobalShortcutsInterface::activated_signal(
            &emitter,
            path.as_ref(),
            shortcut_id,
            timestamp,
            options,
        )
        .await
    }

    async fn emit_deactivated(
        &self,
        session_token: HandleToken,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let emitter = self.emitter()?;
        GlobalShortcutsInterface::deactivated_signal(
            &emitter,
            path.as_ref(),
            shortcut_id,
            timestamp,
            options,
        )
        .await
    }

    async fn emit_shortcuts_changed(
        &self,
        session_token: HandleToken,
        shortcuts: Vec<Shortcut>,
    ) -> zbus::Result<()> {
        let path = self.session_path(&session_token)?;
        let emitter = self.emitter()?;
        GlobalShortcutsInterface::shortcuts_changed_signal(&emitter, path.as_ref(), shortcuts).await
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.GlobalShortcuts")]
impl GlobalShortcutsInterface {
    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        2
    }

    #[zbus(name = "CreateSession")]
    #[zbus(out_args("response", "results"))]
    async fn create_session(
        &self,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: Optional<MaybeAppID>,
        options: CreateSessionOptions,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Response<CreateSessionResponse>> {
        self.inner
            .caller_authorization
            .authorize(&self.inner.cnx, &header)
            .await?;
        let session_token = HandleToken::try_from(&session_handle).unwrap();
        {
            let sessions = self.sessions.lock().unwrap();
            if sessions.contains(&session_token) {
                let errormsg = format!("A session with handle `{session_token}` already exists");
                #[cfg(feature = "tracing")]
                tracing::error!("GlobalShortcutsInterface::create_session: {}", errormsg);
                return Err(PortalError::Exist(errormsg));
            }
        }

        let imp = Arc::clone(&self.imp);
        let token = session_token.clone();
        let result = Request::spawn(
            "GlobalShortcuts::CreateSession",
            &self.inner,
            handle.clone(),
            Arc::clone(&self.imp),
            async move {
                imp.create_session(
                    HandleToken::try_from(&handle).unwrap(),
                    token,
                    app_id.into(),
                    options,
                )
                .await
            },
        )
        .await;

        if result.is_ok() {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                "GlobalShortcutsInterface::create_session: session with handle `{session_token}` created"
            );

            let session = Session::new(
                session_handle,
                Arc::clone(&self.sessions),
                Some(Arc::clone(&self.imp) as Arc<dyn SessionImpl>),
                Arc::clone(&self.inner.caller_authorization),
            );
            session.serve(self.inner.cnx.clone()).await?;
            {
                let mut sessions = self.sessions.lock().unwrap();
                sessions.add(session);
            }
        } else {
            #[cfg(feature = "tracing")]
            tracing::error!(
                "GlobalShortcutsInterface::create_session: failed to create a session with handle `{session_token}`"
            );
        }

        result
    }

    #[zbus(name = "BindShortcuts")]
    #[zbus(out_args("response", "results"))]
    async fn bind_shortcuts(
        &self,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        shortcuts: Vec<NewShortcut>,
        window_identifier: Optional<WindowIdentifierType>,
        options: BindShortcutsOptions,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Response<BindShortcuts>> {
        self.inner
            .caller_authorization
            .authorize(&self.inner.cnx, &header)
            .await?;
        let session_token = HandleToken::try_from(&session_handle).unwrap();
        {
            let sessions = self.sessions.lock().unwrap();
            sessions.try_contains(&session_token)?;
        }

        let imp = Arc::clone(&self.imp);
        Request::spawn(
            "GlobalShortcuts::BindShortcuts",
            &self.inner,
            handle.clone(),
            Arc::clone(&self.imp),
            async move {
                imp.bind_shortcuts(
                    HandleToken::try_from(&handle).unwrap(),
                    session_token,
                    shortcuts,
                    window_identifier.into(),
                    options,
                )
                .await
            },
        )
        .await
    }

    #[zbus(name = "ListShortcuts")]
    #[zbus(out_args("response", "results"))]
    async fn list_shortcuts(
        &self,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Response<ListShortcuts>> {
        self.inner
            .caller_authorization
            .authorize(&self.inner.cnx, &header)
            .await?;
        let session_token = HandleToken::try_from(&session_handle).unwrap();
        {
            let sessions = self.sessions.lock().unwrap();
            sessions.try_contains(&session_token)?;
        }

        let imp = Arc::clone(&self.imp);
        Request::spawn(
            "GlobalShortcuts::ListShortcuts",
            &self.inner,
            handle.clone(),
            Arc::clone(&self.imp),
            async move {
                imp.list_shortcuts(HandleToken::try_from(&handle).unwrap(), session_token)
                    .await
            },
        )
        .await
    }

    #[zbus(name = "ConfigureShortcuts")]
    async fn configure_shortcuts(
        &self,
        session_handle: OwnedObjectPath,
        window_identifier: Optional<WindowIdentifierType>,
        options: ConfigureShortcutsOptions,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.inner
            .caller_authorization
            .authorize(&self.inner.cnx, &header)
            .await?;
        let session_token = HandleToken::try_from(&session_handle).unwrap();
        {
            let sessions = self.sessions.lock().unwrap();
            sessions.try_contains(&session_token)?;
        }

        self.imp
            .configure_shortcuts(session_token, window_identifier.into(), options)
            .await
    }

    #[zbus(signal)]
    #[zbus(name = "Activated")]
    async fn activated_signal(
        emitter: &SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    #[zbus(name = "Deactivated")]
    async fn deactivated_signal(
        emitter: &SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    #[zbus(name = "ShortcutsChanged")]
    async fn shortcuts_changed_signal(
        emitter: &SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        shortcuts: Vec<Shortcut>,
    ) -> zbus::Result<()>;
}
