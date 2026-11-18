//! AppKit menus share the application's action handlers and configurable bindings.
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
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    type Requests = Rc<RefCell<VecDeque<&'static str>>>;
    struct ActionState {
        requested: Requests,
        action: &'static str,
        ctx: egui::Context,
    }
    define_class!(
        #[unsafe(super=NSObject)]
        #[thread_kind=MainThreadOnly]
        #[ivars=ActionState]
        struct MenuTarget;
        unsafe impl NSObjectProtocol for MenuTarget {}
        impl MenuTarget {
            #[unsafe(method(moriMenuAction:))]
            fn invoke(&self, _sender: Option<&AnyObject>) {
                self.ivars().requested.borrow_mut().push_back(self.ivars().action);
                self.ivars().ctx.request_repaint();
            }
        }
    );
    impl MenuTarget {
        fn new(
            mtm: MainThreadMarker,
            requested: Requests,
            action: &'static str,
            ctx: egui::Context,
        ) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(ActionState {
                requested,
                action,
                ctx,
            });
            unsafe { msg_send![super(this), init] }
        }
    }
    struct Entry {
        action: &'static str,
        title: &'static str,
        item: Retained<NSMenuItem>,
        _target: Retained<MenuTarget>,
        last: Option<(bool, String, u64)>,
    }
    impl Entry {
        fn new(
            mtm: MainThreadMarker,
            requested: Requests,
            ctx: egui::Context,
            action: &'static str,
            title: &'static str,
        ) -> Self {
            let target = MenuTarget::new(mtm, requested, action, ctx);
            let item = NSMenuItem::new(mtm);
            unsafe {
                item.setTarget(Some(&target));
                item.setAction(Some(sel!(moriMenuAction:)));
            }
            Self {
                action,
                title,
                item,
                _target: target,
                last: None,
            }
        }
        fn sync(&mut self, chinese: bool, bindings: &[Binding], enabled: bool) {
            let binding = bindings.iter().find(|b| b.action == self.action);
            let key = binding.map_or_else(String::new, |b| key_equivalent(&b.key));
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
            }
            self.item.setEnabled(enabled);
            let identity = (chinese, key.clone(), flags.bits() as u64);
            if self.last.as_ref() == Some(&identity) {
                return;
            }
            self.item
                .setTitle(&NSString::from_str(&i18n::translate(chinese, self.title)));
            self.item.setKeyEquivalent(&NSString::from_str(&key));
            self.item.setKeyEquivalentModifierMask(flags);
            self.last = Some(identity);
        }
    }
    pub struct NativeMenu {
        entries: Vec<Entry>,
        app_menu: Retained<NSMenu>,
        settings_separator: Retained<NSMenuItem>,
        bar: Retained<NSMenu>,
        file_root: Retained<NSMenuItem>,
        requested: Requests,
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
            let app_menu = root.submenu().unwrap_or_else(|| {
                let menu = NSMenu::new(mtm);
                root.setSubmenu(Some(&menu));
                menu
            });
            let requested = Requests::default();
            let settings = Entry::new(mtm, requested.clone(), ctx.clone(), "Settings", "Settings…");
            let index = app_menu.numberOfItems().min(2);
            app_menu.insertItem_atIndex(&settings.item, index);
            let settings_separator = NSMenuItem::separatorItem(mtm);
            app_menu.insertItem_atIndex(&settings_separator, index + 1);
            let file_root = NSMenuItem::new(mtm);
            let file_menu = NSMenu::new(mtm);
            file_menu.setAutoenablesItems(false);
            file_root.setSubmenu(Some(&file_menu));
            bar.insertItem_atIndex(&file_root, 1);
            let mut entries = vec![settings];
            for (action, title) in [
                ("New", "New project"),
                ("Save", "Save"),
                ("Save as", "Save as…"),
                ("Export", "Export video…"),
            ] {
                if action == "Save" || action == "Export" {
                    file_menu.addItem(&NSMenuItem::separatorItem(mtm));
                }
                let entry = Entry::new(mtm, requested.clone(), ctx.clone(), action, title);
                file_menu.addItem(&entry.item);
                entries.push(entry);
            }
            Self {
                entries,
                app_menu,
                settings_separator,
                bar,
                file_root,
                requested,
            }
        }
        pub fn sync(&mut self, chinese: bool, bindings: &[Binding], enabled: bool) {
            self.file_root
                .setTitle(&NSString::from_str(&i18n::translate(chinese, "File")));
            for entry in &mut self.entries {
                entry.sync(chinese, bindings, enabled);
            }
        }
        pub fn take_actions(&self) -> Vec<&'static str> {
            self.requested.borrow_mut().drain(..).collect()
        }
    }
    impl Drop for NativeMenu {
        fn drop(&mut self) {
            self.app_menu.removeItem(&self.entries[0].item);
            self.app_menu.removeItem(&self.settings_separator);
            self.bar.removeItem(&self.file_root);
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
    pub fn sync(&mut self, _chinese: bool, _bindings: &[crate::model::Binding], _enabled: bool) {}
    pub fn take_actions(&self) -> Vec<&'static str> {
        Vec::new()
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
