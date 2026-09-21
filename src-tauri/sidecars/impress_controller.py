#!/usr/bin/env python3
"""
LibreOffice Impress controller sidecar for Metocast.

Communicates via stdin/stdout JSON messages.
Connects to LibreOffice via UNO socket on localhost:2002.

Usage:
    python3 impress_controller.py

Commands (JSON on stdin, one per line):
    {"command": "is_running"}
    {"command": "open", "file_path": "/path/to/file.odp"}
    {"command": "start_slideshow", "from_slide": null}
    {"command": "stop_slideshow"}
    {"command": "next"}
    {"command": "previous"}
    {"command": "goto_slide", "slide_number": 5}
    {"command": "blank_screen"}
    {"command": "white_screen"}
    {"command": "unblank"}
    {"command": "get_status"}
    {"command": "quit"}
"""

import json
import subprocess
import sys
import os

# UNO imports - available when LibreOffice is installed
try:
    import uno
    from com.sun.star.beans import PropertyValue
    HAS_UNO = True
except ImportError:
    HAS_UNO = False

UNO_PORT = 2002


def get_uno_context():
    """Connect to LibreOffice UNO socket."""
    if not HAS_UNO:
        return None
    try:
        local_ctx = uno.getComponentContext()
        resolver = local_ctx.ServiceManager.createInstanceWithContext(
            "com.sun.star.bridge.UnoUrlResolver", local_ctx
        )
        ctx = resolver.resolve(
            f"uno:socket,host=localhost,port={UNO_PORT};urp;StarOffice.ComponentContext"
        )
        return ctx
    except Exception:
        return None


def get_desktop(ctx):
    """Get the LibreOffice desktop from a UNO context."""
    smgr = ctx.ServiceManager
    desktop = smgr.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
    return desktop


def get_slideshow_controller(desktop):
    """Get the active slideshow controller, if any."""
    try:
        doc = desktop.getCurrentComponent()
        if doc is None:
            return None
        presentation = doc.getPresentation()
        if presentation is None or not presentation.isRunning():
            return None
        return presentation.getController()
    except Exception:
        return None


def get_presentation(desktop):
    """Get the presentation object from the current document."""
    try:
        doc = desktop.getCurrentComponent()
        if doc is None:
            return None
        return doc.getPresentation()
    except Exception:
        return None


def respond(success, error=None, **kwargs):
    """Send a JSON response to stdout."""
    resp = {"success": success}
    if error:
        resp["error"] = error
    resp.update(kwargs)
    print(json.dumps(resp), flush=True)


def launch_libreoffice_with_socket(file_path=None):
    """Launch LibreOffice Impress with UNO socket enabled."""
    cmd = [
        "soffice",
        "--impress",
        f"--accept=socket,host=localhost,port={UNO_PORT};urp;",
    ]
    if file_path:
        cmd.append(file_path)
    try:
        subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return True
    except Exception as e:
        return False


def handle_command(cmd_data):
    """Process a single command."""
    command = cmd_data.get("command", "")

    if command == "quit":
        respond(True)
        sys.exit(0)

    if command == "is_running":
        ctx = get_uno_context()
        respond(True, running=ctx is not None)
        return

    if command == "open":
        file_path = cmd_data.get("file_path", "")
        if not os.path.exists(file_path):
            respond(False, error=f"File not found: {file_path}")
            return

        ctx = get_uno_context()
        if ctx is None:
            # Try to launch LibreOffice with socket
            if launch_libreoffice_with_socket(file_path):
                respond(True)
            else:
                respond(False, error="Failed to launch LibreOffice")
            return

        try:
            desktop = get_desktop(ctx)
            url = "file://" + file_path
            props = []
            desktop.loadComponentFromURL(url, "_blank", 0, tuple(props))
            respond(True)
        except Exception as e:
            respond(False, error=str(e))
        return

    # All remaining commands need UNO connection
    ctx = get_uno_context()
    if ctx is None:
        respond(False, error="LibreOffice is not running or UNO socket not available")
        return

    desktop = get_desktop(ctx)

    if command == "start_slideshow":
        try:
            presentation = get_presentation(desktop)
            if presentation is None:
                respond(False, error="No presentation document open")
                return
            from_slide = cmd_data.get("from_slide")
            if from_slide is not None:
                # UNO uses 0-based index for slides
                # But we'll start from beginning and then goto
                presentation.start()
                import time
                time.sleep(0.5)
                controller = get_slideshow_controller(desktop)
                if controller:
                    controller.gotoSlideIndex(from_slide - 1)
            else:
                presentation.start()
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "stop_slideshow":
        try:
            presentation = get_presentation(desktop)
            if presentation and presentation.isRunning():
                presentation.end()
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "next":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            controller.gotoNextSlide()
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "previous":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            controller.gotoPreviousSlide()
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "goto_slide":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            slide_number = cmd_data.get("slide_number", 1)
            controller.gotoSlideIndex(slide_number - 1)  # 0-based
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "blank_screen":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            controller.blankScreen(0x000000)  # Black
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "white_screen":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            controller.blankScreen(0xFFFFFF)  # White
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "unblank":
        try:
            controller = get_slideshow_controller(desktop)
            if controller is None:
                respond(False, error="No slideshow active")
                return
            controller.resume()
            respond(True)
        except Exception as e:
            respond(False, error=str(e))

    elif command == "get_status":
        try:
            doc = desktop.getCurrentComponent()
            slideshow_active = False
            current_slide = None
            total_slides = None
            blanked = False

            if doc is not None:
                try:
                    total_slides = doc.getDrawPages().getCount()
                except Exception:
                    pass

                presentation = get_presentation(desktop)
                if presentation and presentation.isRunning():
                    slideshow_active = True
                    controller = get_slideshow_controller(desktop)
                    if controller:
                        current_slide = controller.getCurrentSlideIndex() + 1  # 1-based
                        blanked = controller.isPaused()

            respond(True, data={
                "slideshow_active": slideshow_active,
                "current_slide": current_slide,
                "total_slides": total_slides,
                "blanked": blanked,
            })
        except Exception as e:
            respond(False, error=str(e))

    else:
        respond(False, error=f"Unknown command: {command}")


def main():
    """Main loop: read JSON commands from stdin, respond on stdout."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            cmd_data = json.loads(line)
            handle_command(cmd_data)
        except json.JSONDecodeError as e:
            respond(False, error=f"Invalid JSON: {e}")
        except Exception as e:
            respond(False, error=f"Unexpected error: {e}")


if __name__ == "__main__":
    main()
