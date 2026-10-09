// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen

// A USB hub on the console's port, for the game loader's USB stack.
//
// The stack knows one device. So a hub is not treated as a device with a
// driver: when the device on the console's port turns out to be a hub, the
// enumeration is diverted here (see usb_host.patch). The hub is set up, its
// ports are switched on, and the first full-speed device found on a port is
// reset. Enumeration then starts again for that device, which gets address 2
// and from there on is the stack's one device. The hub keeps address 1 and
// is not spoken to again.
//
// This lives in the loader's USB library, so games get it without being
// rebuilt, and it uses no memory of its own: its state is in fields of the
// device record that are unused until the device behind the hub is
// enumerated.
//
// Limits: one device, the first one found. Low-speed devices (most wired
// keyboards) are skipped, because the SAMD21 cannot address them through a
// hub. Unplugging the device from the hub is not noticed; reset the console.

#include "usvc_kernel/usvc_kernel.h"

// Hub class requests (USB 2.0, chapter 11.24).
#define HUB_GET_DESCRIPTOR 0xA0
#define HUB_PORT_OUT 0x23
#define HUB_PORT_IN 0xA3
#define HUB_DESCRIPTOR_TYPE 0x29
#define PORT_RESET 4
#define PORT_POWER 8
// Bits of a port status reply, by byte.
#define STATUS0_CONNECTED 0x01
#define STATUS0_ENABLED 0x02
#define STATUS1_LOW_SPEED 0x02
#define CHANGE0_RESET 0x10

// Where the state is kept while the hub is being set up.
#define hubPorts interfacesFound // number of ports
#define hubSkipped iProduct      // bit n: port n holds a device we cannot use
// hubPort is the port being worked on; installationStep is the step below.

void enumerationErrorCallback(void *signal);
void enumTransactionCompleteCallback(void *signal);

// A step that sends a request is followed by a step that waits for it. The
// transfer callback moves on from the waiting step, or sets a negative step
// on failure.
enum
{
	HUB_CONFIGURE,
	HUB_GET_PORTS = HUB_CONFIGURE + 2,
	HUB_COUNT_PORTS = HUB_GET_PORTS + 2,
	HUB_POWER_PORT,
	HUB_PORT_POWERED = HUB_POWER_PORT + 2,
	HUB_PAUSE,
	HUB_NEXT_PORT,
	HUB_PORT_STATUS = HUB_NEXT_PORT + 2,
	HUB_RESET_STARTED = HUB_PORT_STATUS + 2,
	HUB_RESET_WAIT,
	HUB_RESET_STATUS = HUB_RESET_WAIT + 2,
	HUB_RECOVERY,
};

static void hubSend(usbModuleData_t *pUSB, usbDevice_t *pdev, uint8_t type, uint8_t request,
                    uint16_t value, uint16_t length)
{
	usbCreateStandardRequest(pUSB->pUsbEndpoint0ControlBuffer, type, request, value, pdev->hubPort,
	                         length);
	pdev->installationStep++;
	usbAddTransaction(0, length ? pUSB->usbEndpoint0BufferSize : 0, pUSB->pUsbEndpoint0ControlBuffer,
	                  pUSB->pUsbEndpoint0Buffer, length ? DIRECTION_IN : DIRECTION_OUT,
	                  USB_TRANSFER_TYPE_CONTROL, USB_TIMEOUT, enumerationErrorCallback,
	                  enumTransactionCompleteCallback, &pdev->installationStep);
}

static void hubPause(usbDevice_t *pdev, uint32_t nextStep)
{
	pdev->time = millis16();
	pdev->installationStep = nextStep;
}

void usbHubSetup(usbModuleData_t *pUSB, usbDevice_t *pdev)
{
	const uint8_t *reply = pUSB->pUsbEndpoint0Buffer;
	uint16_t elapsed = millis16() - pdev->time;
	switch (pdev->installationStep)
	{
		case HUB_CONFIGURE:
			// A hub has one configuration, and it is number 1. (It is not
			// read from the descriptor: the stack would ask for 256 bytes,
			// which some hubs answer with nothing.)
			hubSend(pUSB, pdev, USB_SETUP_HOST_TO_DEVICE, USB_REQUEST_SET_CONFIGURATION, 1, 0);
			break;
		case HUB_GET_PORTS:
			hubSend(pUSB, pdev, HUB_GET_DESCRIPTOR, USB_REQUEST_GET_DESCRIPTOR, HUB_DESCRIPTOR_TYPE << 8,
			        16);
			break;
		case HUB_COUNT_PORTS:
			pdev->hubPorts = reply[2];
			pdev->installationStep = HUB_POWER_PORT;
			break;
		case HUB_POWER_PORT:
			if (pdev->hubPort == pdev->hubPorts)
			{
				pdev->hubPort = 0;
				hubPause(pdev, HUB_PAUSE);
			}
			else
			{
				pdev->hubPort++;
				hubSend(pUSB, pdev, HUB_PORT_OUT, USB_REQUEST_SET_FEATURE, PORT_POWER, 0);
			}
			break;
		case HUB_PORT_POWERED:
			pdev->installationStep = HUB_POWER_PORT;
			break;
		case HUB_PAUSE:
			// for the ports' power to settle, and between rounds of looking
			if (elapsed >= 300)
				pdev->installationStep = HUB_NEXT_PORT;
			break;
		case HUB_NEXT_PORT:
			if (pdev->hubPort == pdev->hubPorts)
			{
				pdev->hubPort = 0;
				hubPause(pdev, HUB_PAUSE);
			}
			else
			{
				pdev->hubPort++;
				hubSend(pUSB, pdev, HUB_PORT_IN, USB_REQUEST_GET_STATUS, 0, 4);
			}
			break;
		case HUB_PORT_STATUS:
			if ((reply[0] & STATUS0_CONNECTED) && !(pdev->hubSkipped & (1 << pdev->hubPort)))
				hubSend(pUSB, pdev, HUB_PORT_OUT, USB_REQUEST_SET_FEATURE, PORT_RESET, 0);
			else
				pdev->installationStep = HUB_NEXT_PORT;
			break;
		case HUB_RESET_STARTED:
			hubPause(pdev, HUB_RESET_WAIT);
			break;
		case HUB_RESET_WAIT:
			if (elapsed >= 50)
				hubSend(pUSB, pdev, HUB_PORT_IN, USB_REQUEST_GET_STATUS, 0, 4);
			break;
		case HUB_RESET_STATUS:
			if (!(reply[0] & STATUS0_CONNECTED))
				pdev->installationStep = HUB_NEXT_PORT; // unplugged meanwhile
			else if (!(reply[2] & CHANGE0_RESET))
				hubPause(pdev, HUB_RESET_WAIT); // reset still running
			else if (!(reply[0] & STATUS0_ENABLED) || (reply[1] & STATUS1_LOW_SPEED))
			{
				pdev->hubSkipped |= 1 << pdev->hubPort;
				pdev->installationStep = HUB_NEXT_PORT;
			}
			else
				hubPause(pdev, HUB_RECOVERY);
			break;
		case HUB_RECOVERY:
			// The device on this port now answers at address 0. A non-zero
			// hubPort tells the enumeration that it is behind a hub.
			if (elapsed >= 30)
				pdev->enumerationState = FIRST_GET_DD;
			break;
		default:
			if ((int32_t)pdev->installationStep < 0)
				pdev->enumerationState = ENUMERATION_ERROR;
			break;
	}
}
