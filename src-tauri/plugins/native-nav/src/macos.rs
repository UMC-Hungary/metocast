use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSToolbar, NSToolbarDelegate, NSToolbarDisplayMode, NSToolbarItem, NSToolbarItemGroup,
    NSToolbarItemGroupSelectionMode, NSToolbarItemIdentifier, NSWindow, NSWindowToolbarStyle,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString};
use std::cell::Cell;
use tauri::ipc::Channel;
use tauri::{Runtime, WebviewWindow};

use crate::NavItem;

const TOOLBAR_ID: &str = "com.metocast.nav";

fn identifiers() -> Retained<NSArray<NSToolbarItemIdentifier>> {
    NSArray::from_retained_slice(&[NSString::from_str(TOOLBAR_ID)])
}

pub struct Ivars {
    items: Vec<NavItem>,
    active: Cell<isize>,
    on_select: Channel<usize>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MetocastNavToolbarDelegate"]
    #[ivars = Ivars]
    struct NavDelegate;

    unsafe impl NSObjectProtocol for NavDelegate {}

    #[allow(non_snake_case)]
    unsafe impl NSToolbarDelegate for NavDelegate {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn toolbar_itemForItemIdentifier_willBeInsertedIntoToolbar(
            &self,
            _toolbar: &NSToolbar,
            item_identifier: &NSToolbarItemIdentifier,
            _flag: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            (item_identifier.to_string() == TOOLBAR_ID).then(|| self.group())
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbarDefaultItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSToolbarItemIdentifier>> {
            identifiers()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbarAllowedItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSToolbarItemIdentifier>> {
            identifiers()
        }
    }

    impl NavDelegate {
        #[unsafe(method(navSelected:))]
        fn nav_selected(&self, sender: &AnyObject) {
            let index: isize = unsafe { msg_send![sender, selectedIndex] };
            if index >= 0 {
                self.ivars().active.set(index);
                let _ = self.ivars().on_select.send(index as usize);
            }
        }
    }
);

impl NavDelegate {
    fn new(
        mtm: MainThreadMarker,
        items: Vec<NavItem>,
        active: isize,
        on_select: Channel<usize>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            items,
            active: Cell::new(active),
            on_select,
        });
        unsafe { msg_send![super(this), init] }
    }

    fn group(&self) -> Retained<NSToolbarItem> {
        let mtm = MainThreadMarker::from(self);
        let images: Vec<Retained<NSImage>> = self
            .ivars()
            .items
            .iter()
            .filter_map(|item| {
                NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(&item.symbol),
                    Some(&NSString::from_str(&item.label)),
                )
            })
            .collect();
        let labels: Vec<Retained<NSString>> = self
            .ivars()
            .items
            .iter()
            .map(|item| NSString::from_str(&item.label))
            .collect();

        let group = unsafe {
            NSToolbarItemGroup::groupWithItemIdentifier_images_selectionMode_labels_target_action(
                &NSString::from_str(TOOLBAR_ID),
                &NSArray::from_retained_slice(&images),
                NSToolbarItemGroupSelectionMode::SelectOne,
                Some(&NSArray::from_retained_slice(&labels)),
                Some(self as &AnyObject),
                Some(sel!(navSelected:)),
                mtm,
            )
        };
        group.setSelectedIndex(self.ivars().active.get());
        Retained::into_super(group)
    }
}

fn with_window<R: Runtime>(
    window: &WebviewWindow<R>,
    f: impl FnOnce(&NSWindow, MainThreadMarker) + Send + 'static,
) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ptr = ptr as usize;
    let _ = window.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let ns_window: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
        f(ns_window, mtm);
    });
}

pub fn install<R: Runtime>(
    window: &WebviewWindow<R>,
    items: Vec<NavItem>,
    active: usize,
    on_select: Channel<usize>,
) {
    with_window(window, move |ns_window, mtm| {
        let delegate = NavDelegate::new(mtm, items, active as isize, on_select);
        let toolbar =
            NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str(TOOLBAR_ID));
        toolbar.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        toolbar.setAllowsUserCustomization(false);
        toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
        ns_window.setToolbarStyle(NSWindowToolbarStyle::Unified);
        ns_window.setToolbar(Some(&toolbar));
        // The toolbar holds its delegate weakly; nothing else owns it for the app's lifetime.
        std::mem::forget(delegate);
    });
}

pub fn set_active<R: Runtime>(window: &WebviewWindow<R>, active: usize) {
    with_window(window, move |ns_window, _mtm| {
        let Some(toolbar) = ns_window.toolbar() else {
            return;
        };
        for item in toolbar.items().iter() {
            if let Ok(group) = item.downcast::<NSToolbarItemGroup>() {
                group.setSelectedIndex(active as isize);
            }
        }
    });
}
