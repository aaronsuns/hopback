//! Accessibility: read VS Code's Window menu (it lists every window, on every
//! Space) and raise a window by pressing its menu item.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

type AXUIElementRef = CFTypeRef;
type AXError = i32;
const AX_OK: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> AXError;
    fn AXUIElementPerformAction(el: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> AXError;
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask macOS to show its "allow Accessibility" prompt (adds Hopback to the list).
pub fn prompt_for_trust() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()) }
}

struct Element(CFType);

impl Element {
    fn app(pid: i32) -> Option<Element> {
        let r = unsafe { AXUIElementCreateApplication(pid) };
        if r.is_null() {
            return None;
        }
        unsafe { AXUIElementSetMessagingTimeout(r, 1.0) };
        Some(Element(unsafe { CFType::wrap_under_create_rule(r) }))
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef()
    }

    fn attr(&self, name: &str) -> Option<CFType> {
        let mut v: CFTypeRef = std::ptr::null();
        let err =
            unsafe { AXUIElementCopyAttributeValue(self.raw(), CFString::new(name).as_concrete_TypeRef(), &mut v) };
        (err == AX_OK && !v.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(v) })
    }

    fn element(&self, name: &str) -> Option<Element> {
        self.attr(name).map(Element)
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attr(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    fn children(&self) -> Vec<Element> {
        let Some(v) = self.attr("AXChildren") else { return Vec::new() };
        let arr: CFArray<*const c_void> = unsafe { CFArray::wrap_under_get_rule(v.as_CFTypeRef() as CFArrayRef) };
        arr.iter().map(|p| Element(unsafe { CFType::wrap_under_get_rule(*p) })).collect()
    }

    fn press(&self) -> bool {
        unsafe { AXUIElementPerformAction(self.raw(), CFString::new("AXPress").as_concrete_TypeRef()) == AX_OK }
    }

    fn set_bool(&self, name: &str, value: bool) -> bool {
        let v = if value { CFBoolean::true_value() } else { CFBoolean::false_value() };
        unsafe {
            AXUIElementSetAttributeValue(self.raw(), CFString::new(name).as_concrete_TypeRef(), v.as_CFTypeRef())
                == AX_OK
        }
    }
}

/// The Window menu's items. VS Code's menu titles are English unless its display
/// language is changed; then the menu just before Help is used.
fn window_menu_items(pid: i32) -> Option<Vec<Element>> {
    let app = Element::app(pid)?;
    let bar = app.element("AXMenuBar")?;
    let items = bar.children();
    let idx = items
        .iter()
        .position(|i| i.string("AXTitle").as_deref() == Some("Window"))
        .or_else(|| items.len().checked_sub(2))?;
    let menu = items[idx].children().into_iter().next()?;
    Some(menu.children())
}

fn titles(items: &[Element]) -> Vec<Option<String>> {
    items.iter().map(|i| i.string("AXTitle").filter(|t| !t.is_empty())).collect()
}

/// `(menu index, window title)` for every VS Code window.
pub fn list_windows(pid: i32) -> Option<Vec<(usize, String)>> {
    let items = window_menu_items(pid)?;
    let titles = titles(&items);
    Some(
        hopback_core::windows::window_item_indices(&titles)
            .into_iter()
            .filter_map(|i| titles[i].clone().map(|t| (i, t)))
            .collect(),
    )
}

/// Raise the window whose menu item is at `index` with title `title`
/// (re-checked, since the menu may have changed), then bring VS Code forward.
pub fn raise(pid: i32, index: usize, title: &str) -> bool {
    let Some(items) = window_menu_items(pid) else { return false };
    let titles = titles(&items);
    let pos = if titles.get(index).and_then(|t| t.as_deref()) == Some(title) {
        Some(index)
    } else {
        titles.iter().position(|t| t.as_deref() == Some(title))
    };
    let Some(pos) = pos else { return false };
    let ok = items[pos].press();
    if let Some(app) = Element::app(pid) {
        app.set_bool("AXFrontmost", true);
    }
    ok
}

/// Title of VS Code's focused window, if VS Code is the frontmost app.
pub fn focused_window_title(pid: i32) -> Option<String> {
    let app = Element::app(pid)?;
    let front = app.attr("AXFrontmost").and_then(|v| v.downcast::<CFBoolean>()).map(bool::from).unwrap_or(false);
    if !front {
        return None;
    }
    app.element("AXFocusedWindow")?.string("AXTitle")
}

/// Pid of the running VS Code (stable) main process, found without spawning.
pub fn vscode_pid() -> Option<i32> {
    let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if n <= 0 {
        return None;
    }
    let mut pids = vec![0i32; n as usize + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr() as *mut c_void, bytes) };
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    pids.truncate(n.max(0) as usize);
    pids.into_iter().find(|&pid| {
        let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        if len <= 0 {
            return false;
        }
        let path = String::from_utf8_lossy(&buf[..len as usize]);
        path.ends_with("/Visual Studio Code.app/Contents/MacOS/Code")
            || path.ends_with("/Visual Studio Code.app/Contents/MacOS/Electron")
    })
}
