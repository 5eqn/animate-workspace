//! Adds Settings to the existing AppKit application menu without replacing its
//! About, Services, Hide or Quit items. The retained target owns the action.
#[cfg(target_os = "macos")]
mod mac {
    use super::key_equivalent;
    use crate::{i18n, model::Binding};
    use eframe::egui;
    use objc2::{
        define_class, msg_send, rc::Retained, runtime::AnyObject, sel, DefinedClass,
        MainThreadMarker, MainThreadOnly,
    };
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
    use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
    use std::{cell::Cell, rc::Rc};
    struct ActionState {
        requested: Rc<Cell<bool>>,
        ctx: egui::Context,
    }
    define_class!(
        #[unsafe(super=NSObject)]
        #[thread_kind=MainThreadOnly]
        #[ivars=ActionState]
        struct SettingsTarget;
        unsafe impl NSObjectProtocol for SettingsTarget{}
        impl SettingsTarget{
            #[unsafe(method(moriOpenSettings:))]
            fn open_settings(&self,_sender:Option<&AnyObject>){self.ivars().requested.set(true);self.ivars().ctx.request_repaint();}
        }
    );
    impl SettingsTarget {
        fn new(
            mtm: MainThreadMarker,
            requested: Rc<Cell<bool>>,
            ctx: egui::Context,
        ) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(ActionState { requested, ctx });
            unsafe { msg_send![super(this), init] }
        }
    }
    pub struct NativeMenu {
        item: Retained<NSMenuItem>,
        menu: Retained<NSMenu>,
        _target: Retained<SettingsTarget>,
        requested: Rc<Cell<bool>>,
        last: Option<(bool, String, u64)>,
    }
    impl NativeMenu {
        pub fn new(ctx: egui::Context) -> Self {
            let mtm = MainThreadMarker::new().expect("AppKit UI thread");
            let app = NSApplication::sharedApplication(mtm);
            let bar = app.mainMenu().unwrap_or_else(|| {
                let bar = NSMenu::new(mtm);
                app.setMainMenu(Some(&bar));
                bar
            });
            let root = bar.itemAtIndex(0).unwrap_or_else(|| {
                let root = NSMenuItem::new(mtm);
                root.setTitle(&NSString::from_str("MORI"));
                bar.addItem(&root);
                root
            });
            let menu = root.submenu().unwrap_or_else(|| {
                let menu = NSMenu::new(mtm);
                root.setSubmenu(Some(&menu));
                menu
            });
            let requested = Rc::new(Cell::new(false));
            let target = SettingsTarget::new(mtm, requested.clone(), ctx);
            let item = NSMenuItem::new(mtm);
            item.setEnabled(true);
            unsafe {
                item.setTarget(Some(&target));
                item.setAction(Some(sel!(moriOpenSettings:)));
            }
            let index = menu.numberOfItems().min(2);
            menu.insertItem_atIndex(&item, index);
            menu.insertItem_atIndex(&NSMenuItem::separatorItem(mtm), index + 1);
            Self {
                item,
                menu,
                _target: target,
                requested,
                last: None,
            }
        }
        pub fn sync(&mut self, chinese: bool, binding: Option<&Binding>) {
            let key = binding.map_or_else(|| ",".into(), |b| key_equivalent(&b.key));
            let mut flags = NSEventModifierFlags::empty();
            if let Some(b) = binding {
                if b.command {
                    flags |= NSEventModifierFlags::Command;
                }
                if b.control {
                    flags |= NSEventModifierFlags::Control;
                }
                if b.alt {
                    flags |= NSEventModifierFlags::Option;
                }
                if b.shift {
                    flags |= NSEventModifierFlags::Shift;
                }
            } else {
                flags = NSEventModifierFlags::Command;
            }
            let identity = (chinese, key.clone(), flags.bits() as u64);
            if self.last.as_ref() == Some(&identity) {
                return;
            }
            self.item
                .setTitle(&NSString::from_str(&i18n::translate(chinese, "Settings…")));
            self.item.setKeyEquivalent(&NSString::from_str(&key));
            self.item.setKeyEquivalentModifierMask(flags);
            self.last = Some(identity);
        }
        pub fn take_settings(&self) -> bool {
            self.requested.replace(false)
        }
    }
    impl Drop for NativeMenu {
        fn drop(&mut self) {
            self.menu.removeItem(&self.item);
        }
    }
}
#[cfg(target_os = "macos")]
pub use mac::NativeMenu;
#[cfg(not(target_os = "macos"))]
pub struct NativeMenu;
#[cfg(not(target_os = "macos"))]
impl NativeMenu {
    pub fn new(_ctx: eframe::egui::Context) -> Self {
        Self
    }
    pub fn sync(&mut self, _chinese: bool, _binding: Option<&crate::model::Binding>) {}
    pub fn take_settings(&self) -> bool {
        false
    }
}

#[cfg(any(target_os = "macos", test))]
fn key_equivalent(key: &str) -> String {
    use eframe::egui::Key;
    let Some(key) = Key::from_name(key) else {
        return String::new();
    };
    let special = match key {
        Key::Space => ' ',
        Key::Enter => '\r',
        Key::Escape => '\u{1b}',
        Key::Tab => '\t',
        Key::Backspace => '\u{8}',
        Key::ArrowLeft => '\u{f702}',
        Key::ArrowRight => '\u{f703}',
        Key::ArrowUp => '\u{f700}',
        Key::ArrowDown => '\u{f701}',
        Key::Insert => '\u{f727}',
        Key::Delete => '\u{f728}',
        Key::Home => '\u{f729}',
        Key::End => '\u{f72b}',
        Key::PageUp => '\u{f72c}',
        Key::PageDown => '\u{f72d}',
        Key::Minus => '-',
        Key::Quote => '\'',
        _ => {
            if let Some(number) = key
                .name()
                .strip_prefix('F')
                .and_then(|n| n.parse::<u32>().ok())
            {
                return char::from_u32(0xf704 + number - 1).unwrap().into();
            }
            let symbol = key.symbol_or_name();
            return if symbol.chars().count() == 1 {
                symbol.to_lowercase()
            } else {
                String::new()
            };
        }
    };
    special.into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_keys_follow_configurable_shortcut_names_and_symbols() {
        assert_eq!(key_equivalent(","), ",");
        assert_eq!(key_equivalent("Comma"), ",");
        assert_eq!(key_equivalent("Z"), "z");
        assert_eq!(key_equivalent("⏶"), "\u{f700}");
        assert_eq!(key_equivalent("PageDown"), "\u{f72d}");
        assert_eq!(key_equivalent("F12"), "\u{f70f}");
        assert_eq!(key_equivalent("−"), "-");
        assert_eq!(key_equivalent("Quote"), "'");
    }
}
