// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen

// USB hub probe for a real uSVC console.
//
// It runs in the game loader's place (at 0x2000) and uses the loader's
// kernel and USB host stack. It answers one question: can this console talk
// to a full-speed device that sits behind a USB hub?
//
// Two drivers are registered with the stack. The hub driver sets up a hub,
// switches its ports on, and when a port reports a device it resets that
// port and asks the stack to enumerate the device behind it. The probe
// driver accepts any other device and configures it. It shows one of them
// and reads its first interrupt IN endpoint: the first device found, or a
// HID device if one turns up later (some hubs have devices of their own
// built in). A report counter that goes up while the device is on a hub port
// means yes.
//
// Not handled: devices unplugged from the hub and plugged in again (press
// the reset button), hubs behind hubs, and low-speed devices behind the hub,
// which the SAMD21 cannot address.

#include "usvc_kernel/usvc_kernel.h"

#define NUMBER_OF_CHARACTERS (128 + 6 - ' ')
#define MAX_HUB_PORTS 4

// Hub class requests (USB 2.0, chapter 11.24).
#define HUB_GET_DESCRIPTOR 0xA0
#define HUB_PORT_OUT 0x23
#define HUB_PORT_IN 0xA3
#define HUB_DESCRIPTOR_TYPE 0x29
#define PORT_RESET 4
#define PORT_POWER 8
#define C_PORT_CONNECTION 16
#define C_PORT_RESET 20
#define STATUS_CONNECTED 0x0001
#define STATUS_ENABLED 0x0002
#define STATUS_POWERED 0x0100
#define STATUS_LOW_SPEED 0x0200
#define CHANGE_RESET 0x0010

#define STEP_ERROR 0xFFFF

extern uint32_t *_sfixed;
// As in the loader: the vector table is copied to RAM.
void *ramVectorTable[16 + 44] __attribute__((__aligned__(256)));

// Both drivers start by reading the configuration descriptor again. The
// stack has just read it, but it asks for 256 bytes, and a device that only
// looks at the low byte of the length (the 214B:7250 hub does) sends none.
#define CONFIGURATION_REQUEST_LENGTH 255

enum
{
	HI_GET_CONFIGURATION,
	HI_WAIT_GET_CONFIGURATION,
	HI_SET_CONFIGURATION,
	HI_WAIT_SET_CONFIGURATION,
	HI_GET_DESCRIPTOR,
	HI_WAIT_GET_DESCRIPTOR,
	HI_PARSE_DESCRIPTOR,
	HI_POWER_PORT,
	HI_WAIT_POWER_PORT,
	HI_NEXT_PORT,
};

// Each ..._WAIT state is left by the transfer callback, which adds one.
enum
{
	H_PAUSE,
	H_STATUS_SEND,
	H_STATUS_WAIT,
	H_STATUS_DONE,
	H_CLEAR_CONNECTION_SEND,
	H_CLEAR_CONNECTION_WAIT,
	H_CLEAR_CONNECTION_DONE,
	H_DEBOUNCE,
	H_RESET_SEND,
	H_RESET_WAIT,
	H_RESET_DONE,
	H_RESET_DELAY,
	H_RESET_STATUS_SEND,
	H_RESET_STATUS_WAIT,
	H_RESET_STATUS_DONE,
	H_CLEAR_RESET_SEND,
	H_CLEAR_RESET_WAIT,
	H_CLEAR_RESET_DONE,
	H_RECOVERY,
	H_HUB_STATUS_SEND,
	H_HUB_STATUS_WAIT,
	H_HUB_STATUS_DONE,
	H_CHILD,
};

typedef struct
{
	uint32_t state;
	uint16_t time;
	uint16_t pause;
	uint16_t portStatus[MAX_HUB_PORTS + 1];
	uint16_t portChange[MAX_HUB_PORTS + 1];
	uint8_t handled[MAX_HUB_PORTS + 1];
	uint8_t ready;
	uint8_t address;
	uint8_t ep0Size;
	uint8_t ports;
	uint8_t port;
	uint8_t tries;
	uint8_t childIndex;
} hub_t;

typedef struct
{
	__attribute__((__aligned__(4))) uint8_t report[64];
	uint16_t time;
	uint16_t idVendor;
	uint16_t idProduct;
	uint16_t reports;
	uint16_t maxPacket;
	uint8_t seen;  // something to show
	uint8_t bound; // a device is bound since the stack last started over
	uint8_t ready;
	uint8_t address;
	uint8_t hubPort;
	uint8_t deviceClass;
	uint8_t interfaceClass;
	uint8_t interfaceSubClass;
	uint8_t interfaceProtocol;
	uint8_t endpoint;
	uint8_t interval;
	uint8_t pipe;
	uint8_t lastLength;
	uint8_t enumerationStep;
} probe_t;

static hub_t hub;
static probe_t probe;
static char rootName[32];
static char deviceName[32];
static const char *note = "";
static uint16_t restarts;
static uint16_t rootVendor;
static uint16_t rootProduct;
static uint8_t rootClass;
static uint8_t rootSeen;
// Shown raw, to tell a hub that answers with zeros from one that does not
// answer: the hub descriptor, the configuration it was given, and the size
// and number of the port status replies.
static uint8_t hubDescriptor[9];
static uint8_t hubConfiguration;
static uint8_t statusReply[4];
static uint8_t hubStatus[4]; // bit 1 of byte 0: over-current
static uint8_t lastBytesReceived;
static uint16_t statusPolls;

// ---------------------------------------------------------------- screen

static void print(const char *text, uint8_t col, uint8_t row)
{
	for (; col < 40 && *text; col++)
		vram[row * VRAMX + col] = *text++ - ' ';
}

static void clearRow(uint8_t row)
{
	memset(&vram[row * VRAMX], 0, 40);
}

static void printHex(uint32_t value, uint8_t digits, uint8_t col, uint8_t row)
{
	for (int i = digits - 1; i >= 0; i--)
	{
		vram[row * VRAMX + col + i] = "0123456789ABCDEF"[value & 15] - ' ';
		value >>= 4;
	}
}

static void printDecimal(uint16_t value, uint8_t col, uint8_t row)
{
	for (int i = 4; i >= 0; i--)
	{
		vram[row * VRAMX + col + i] = '0' + value % 10 - ' ';
		value /= 10;
	}
}

// ------------------------------------------------------------- callbacks

static void stepComplete(void *signal, int bytesReceived)
{
	lastBytesReceived = bytesReceived;
	*(uint32_t *)signal += 1;
}

static void stepError(void *signal)
{
	*(uint32_t *)signal = STEP_ERROR;
}

static void productNameFound(uint8_t *name)
{
	char *destination = getUSBData()->currentUSBdeviceNumber == 0 ? rootName : deviceName;
	memset(destination, 0, 32);
	if (name)
		strncpy(destination, (char *)name, 31);
}

// A control request to whichever device pipe 0 is currently set up for.
static uint32_t controlRequest(uint8_t type, uint8_t request, uint16_t value, uint16_t index,
                               uint16_t length, uint32_t *step)
{
	usbModuleData_t *pUSB = getUSBData();
	usbCreateStandardRequest(pUSB->pUsbEndpoint0ControlBuffer, type, request, value, index, length);
	if (type & USB_SETUP_DEVICE_TO_HOST)
	{
		memset(pUSB->pUsbEndpoint0Buffer, 0, 16);
		return usbAddTransaction(0, pUSB->usbEndpoint0BufferSize, pUSB->pUsbEndpoint0ControlBuffer,
		                         pUSB->pUsbEndpoint0Buffer, DIRECTION_IN, USB_TRANSFER_TYPE_CONTROL,
		                         USB_TIMEOUT, stepError, stepComplete, step);
	}
	return usbAddTransaction(0, 0, pUSB->pUsbEndpoint0ControlBuffer, pUSB->pUsbEndpoint0Buffer,
	                         DIRECTION_OUT, USB_TRANSFER_TYPE_CONTROL, USB_TIMEOUT, stepError,
	                         stepComplete, step);
}

// For the installers, which have no way to try again: a request that cannot
// be queued is a failed step.
static void installRequest(uint8_t type, uint8_t request, uint16_t value, uint16_t index,
                           uint16_t length, uint32_t *step)
{
	if (controlRequest(type, request, value, index, length, step))
		*step = STEP_ERROR;
}

// ------------------------------------------------------------ hub driver

static uint32_t hubInstaller(uint32_t action, usbDevice_t *pdev, void *paramPtr)
{
	(void)paramPtr;
	usbModuleData_t *pUSB = getUSBData();
	if (action == CHECK_DRIVER_COMPATIBILITY)
	{
		// Only as the device on the console's own port: no hubs behind hubs.
		if (pdev->bDeviceClass == USB_CLASS_HUB && pUSB->currentUSBdeviceNumber == 0)
			return USB_DRIVER_FOUND;
		return USB_DRIVER_NOT_COMPATIBLE;
	}
	if (action == DEVICE_INSTALLED_CALLBACK)
	{
		hub.ready = 1;
		hub.state = H_PAUSE;
		hub.pause = 300; // let the ports' power settle
		hub.time = millis16();
		hub.port = 1;
		return 0;
	}
	if (action != INSTALL_DEVICE)
		return 0; // no driver memory needed
	switch (pdev->installationStep)
	{
		case HI_GET_CONFIGURATION:
			hub.address = pdev->address;
			hub.ep0Size = pdev->ep0Size;
			pdev->installationStep = HI_WAIT_GET_CONFIGURATION;
			installRequest(USB_SETUP_DEVICE_TO_HOST, USB_REQUEST_GET_DESCRIPTOR,
			               USB_DESCRIPTOR_CONFIGURATION << 8, 0, CONFIGURATION_REQUEST_LENGTH,
			               &pdev->installationStep);
			break;
		case HI_SET_CONFIGURATION:
			pdev->installationStep = HI_WAIT_SET_CONFIGURATION;
			// Byte 5 of the configuration descriptor. If that still came back
			// empty, try 1, which is what nearly every device uses.
			hubConfiguration = pUSB->pUsbEndpoint0Buffer[5];
			installRequest(USB_SETUP_HOST_TO_DEVICE, USB_REQUEST_SET_CONFIGURATION,
			               hubConfiguration ? hubConfiguration : 1, 0, 0, &pdev->installationStep);
			break;
		case HI_GET_DESCRIPTOR:
			pdev->installationStep = HI_WAIT_GET_DESCRIPTOR;
			installRequest(HUB_GET_DESCRIPTOR, USB_REQUEST_GET_DESCRIPTOR, HUB_DESCRIPTOR_TYPE << 8, 0,
			               16, &pdev->installationStep);
			break;
		case HI_PARSE_DESCRIPTOR:
			memcpy(hubDescriptor, pUSB->pUsbEndpoint0Buffer, sizeof(hubDescriptor));
			hub.ports = pUSB->pUsbEndpoint0Buffer[2];
			if (hub.ports > MAX_HUB_PORTS)
				hub.ports = MAX_HUB_PORTS;
			hub.port = 1;
			pdev->installationStep = hub.ports ? HI_POWER_PORT : STEP_ERROR;
			break;
		case HI_POWER_PORT:
			pdev->installationStep = HI_WAIT_POWER_PORT;
			installRequest(HUB_PORT_OUT, USB_REQUEST_SET_FEATURE, PORT_POWER, hub.port, 0,
			               &pdev->installationStep);
			break;
		case HI_NEXT_PORT:
			if (++hub.port <= hub.ports)
				pdev->installationStep = HI_POWER_PORT;
			else
				pdev->enumerationState = ENUMERATION_COMPLETE;
			break;
		case STEP_ERROR:
			note = "hub set-up failed";
			pdev->enumerationState = ENUMERATION_ERROR;
			break;
	}
	return 0;
}

// Send a request to the hub, about the current port unless the request type
// is for the hub itself. Stay in the same state and try again if pipe 0 is
// busy.
static void hubSend(uint8_t type, uint8_t request, uint16_t value, uint16_t length)
{
	uint32_t sendState = hub.state;
	uint16_t index = type == HUB_GET_DESCRIPTOR ? 0 : hub.port;
	hub.state = sendState + 1;
	uhdPipe0Alloc(hub.address, hub.ep0Size);
	if (controlRequest(type, request, value, index, length, &hub.state))
		hub.state = sendState;
}

static void hubNextPort(void)
{
	if (++hub.port > hub.ports)
	{
		hub.port = 1;
		hub.state = H_HUB_STATUS_SEND; // then a pause, then round again
	}
	else
		hub.state = H_STATUS_SEND;
}

static void hubGiveUpOnPort(const char *why)
{
	note = why;
	hub.handled[hub.port] = 1;
	hubNextPort();
}

static void hubReadStatus(void)
{
	uint8_t *p = getUSBData()->pUsbEndpoint0Buffer;
	memcpy(statusReply, p, sizeof(statusReply));
	statusPolls++;
	hub.portStatus[hub.port] = p[0] | (p[1] << 8);
	hub.portChange[hub.port] = p[2] | (p[3] << 8);
}

static void hubTask(void)
{
	usbModuleData_t *pUSB = getUSBData();
	if (!hub.ready)
		return;
	if (hub.state == H_CHILD)
	{
		uint32_t step = pUSB->pUsbDevices[hub.childIndex].enumerationState;
		if (step < 0x80 && step > probe.enumerationStep)
			probe.enumerationStep = step;
		if (pUSB->usbTaskState == USB_STATE_RUNNING)
			hubNextPort();
		return;
	}
	if (pUSB->usbTaskState != USB_STATE_RUNNING)
		return;
	uint16_t elapsed = millis16() - hub.time;
	switch (hub.state)
	{
		case H_PAUSE:
			if (elapsed >= hub.pause)
				hub.state = H_STATUS_SEND;
			break;
		case H_STATUS_SEND:
		case H_RESET_STATUS_SEND:
			hubSend(HUB_PORT_IN, USB_REQUEST_GET_STATUS, 0, 4);
			break;
		case H_STATUS_DONE:
			hubReadStatus();
			if ((hub.portStatus[hub.port] & STATUS_CONNECTED) && !hub.handled[hub.port])
				hub.state = H_CLEAR_CONNECTION_SEND;
			else
				hubNextPort();
			break;
		case H_CLEAR_CONNECTION_SEND:
			hubSend(HUB_PORT_OUT, USB_REQUEST_CLEAR_FEATURE, C_PORT_CONNECTION, 0);
			break;
		case H_CLEAR_CONNECTION_DONE:
			hub.time = millis16();
			hub.state = H_DEBOUNCE;
			break;
		case H_DEBOUNCE:
			if (elapsed >= 120)
				hub.state = H_RESET_SEND;
			break;
		case H_RESET_SEND:
			hubSend(HUB_PORT_OUT, USB_REQUEST_SET_FEATURE, PORT_RESET, 0);
			break;
		case H_RESET_DONE:
			hub.tries = 0;
			hub.time = millis16();
			hub.state = H_RESET_DELAY;
			break;
		case H_RESET_DELAY:
			if (elapsed >= 40)
				hub.state = H_RESET_STATUS_SEND;
			break;
		case H_RESET_STATUS_DONE:
			hubReadStatus();
			if (hub.portChange[hub.port] & CHANGE_RESET)
				hub.state = H_CLEAR_RESET_SEND;
			else if (++hub.tries > 20)
				hubGiveUpOnPort("port reset did not finish");
			else
			{
				hub.time = millis16();
				hub.state = H_RESET_DELAY;
			}
			break;
		case H_CLEAR_RESET_SEND:
			hubSend(HUB_PORT_OUT, USB_REQUEST_CLEAR_FEATURE, C_PORT_RESET, 0);
			break;
		case H_CLEAR_RESET_DONE:
			hub.time = millis16();
			hub.state = H_RECOVERY;
			break;
		case H_RECOVERY:
			if (elapsed < 30)
				break;
			if (!(hub.portStatus[hub.port] & STATUS_ENABLED))
				hubGiveUpOnPort("port not enabled after reset");
			else if (hub.portStatus[hub.port] & STATUS_LOW_SPEED)
				hubGiveUpOnPort("low-speed device: not possible");
			else
			{
				uint8_t address = 0;
				if (usbStartHubEnumeration(&address, hub.port))
					hubGiveUpOnPort("no free device slot");
				else
				{
					hub.handled[hub.port] = 1;
					hub.childIndex = address - 1;
					probe.enumerationStep = 0;
					note = "enumerating device behind hub";
					hub.state = H_CHILD;
				}
			}
			break;
		case H_HUB_STATUS_SEND:
			// same request type as the hub descriptor: class, to the device
			hubSend(HUB_GET_DESCRIPTOR, USB_REQUEST_GET_STATUS, 0, 4);
			break;
		case H_HUB_STATUS_DONE:
			memcpy(hubStatus, pUSB->pUsbEndpoint0Buffer, sizeof(hubStatus));
			hub.pause = 200;
			hub.time = millis16();
			hub.state = H_PAUSE;
			break;
		case STEP_ERROR:
			// The stack leaves a failed pipe unusable, so start over.
			note = "hub request failed";
			pUSB->usbTaskState = USB_STATE_ERROR;
			hub.ready = 0;
			break;
	}
}

// ---------------------------------------------------------- probe driver

enum
{
	PI_GET_CONFIGURATION,
	PI_WAIT_GET_CONFIGURATION,
	PI_SET_CONFIGURATION,
	PI_WAIT_SET_CONFIGURATION,
	PI_OPEN_PIPE,
};

static void probeParseConfiguration(const uint8_t *p, uint16_t size)
{
	uint16_t total = p[2] | (p[3] << 8);
	uint8_t interfaceFound = 0;
	if (total > size)
		total = size;
	probe.endpoint = 0;
	for (uint16_t n = 0; n + 6 < total && p[n]; n += p[n])
	{
		if (p[n + 1] == USB_DESCRIPTOR_INTERFACE && !interfaceFound)
		{
			interfaceFound = 1;
			probe.interfaceClass = p[n + 5];
			probe.interfaceSubClass = p[n + 6];
			probe.interfaceProtocol = p[n + 7];
		}
		else if (p[n + 1] == USB_DESCRIPTOR_ENDPOINT && !probe.endpoint && (p[n + 2] & 0x80) &&
		         (p[n + 3] & 3) == USB_TRANSFER_TYPE_INTERRUPT)
		{
			probe.endpoint = p[n + 2] & 0xF;
			probe.maxPacket = p[n + 4] | (p[n + 5] << 8);
			probe.interval = p[n + 6];
		}
	}
}

static uint32_t probeInstaller(uint32_t action, usbDevice_t *pdev, void *paramPtr)
{
	(void)paramPtr;
	usbModuleData_t *pUSB = getUSBData();
	if (action == CHECK_DRIVER_COMPATIBILITY)
		return USB_DRIVER_FOUND; // anything that is not a hub
	if (action == DEVICE_INSTALLED_CALLBACK)
	{
		if (probe.address != pdev->address)
			return 0;
		probe.ready = 1;
		probe.time = millis16();
		note = probe.endpoint ? "device set up" : "device has no interrupt endpoint";
		return 0;
	}
	if (action != INSTALL_DEVICE)
		return 0; // no driver memory needed
	switch (pdev->installationStep)
	{
		case PI_GET_CONFIGURATION:
			pdev->installationStep = PI_WAIT_GET_CONFIGURATION;
			installRequest(USB_SETUP_DEVICE_TO_HOST, USB_REQUEST_GET_DESCRIPTOR,
			               USB_DESCRIPTOR_CONFIGURATION << 8, 0, CONFIGURATION_REQUEST_LENGTH,
			               &pdev->installationStep);
			break;
		case PI_SET_CONFIGURATION:
		{
			const uint8_t *configuration = pUSB->pUsbEndpoint0Buffer;
			// byte 14 is the class of the first interface
			uint8_t isHid = configuration[14] == USB_CLASS_HID;
			if (!probe.bound || (isHid && probe.interfaceClass != USB_CLASS_HID))
			{
				if (probe.bound && probe.pipe)
					uhdPipeFree(probe.pipe);
				probe.seen = 1;
				probe.bound = 1;
				probe.ready = 0;
				probe.pipe = 0;
				probe.reports = 0;
				probe.address = pdev->address;
				probe.hubPort = pdev->hubPort;
				probe.idVendor = pdev->idVendor;
				probe.idProduct = pdev->idProduct;
				probe.deviceClass = pdev->bDeviceClass;
				probeParseConfiguration(configuration, pUSB->usbEndpoint0BufferSize);
			}
			pdev->installationStep = PI_WAIT_SET_CONFIGURATION;
			// byte 5 of the configuration descriptor
			installRequest(USB_SETUP_HOST_TO_DEVICE, USB_REQUEST_SET_CONFIGURATION, configuration[5],
			               0, 0, &pdev->installationStep);
			break;
		}
		case PI_OPEN_PIPE:
			if (probe.address == pdev->address && probe.endpoint)
			{
				// the pipe size must be a power of two
				uint16_t size = 8;
				while (size < probe.maxPacket && size < 64)
					size <<= 1;
				probe.maxPacket = size;
				probe.pipe = uhdPipeAlloc(probe.address, probe.endpoint, USB_HOST_PTYPE_INT_val,
				                          USB_EP_DIR_IN, size, probe.interval, 0);
			}
			pdev->enumerationState = ENUMERATION_COMPLETE;
			break;
		case STEP_ERROR:
			note = "device set-up failed";
			pdev->enumerationState = ENUMERATION_ERROR;
			break;
	}
	return 0;
}

static void reportReceived(void *signal, int bytesReceived)
{
	(void)signal;
	probe.reports++;
	probe.lastLength = bytesReceived;
}

static void probePoll(void)
{
	if (!probe.ready || !probe.pipe || getUSBData()->usbTaskState != USB_STATE_RUNNING)
		return;
	if ((uint16_t)(millis16() - probe.time) < (probe.interval < 4 ? 4 : probe.interval))
		return;
	// No timeout: a device with nothing new to say may stay silent.
	if (usbAddTransaction(probe.pipe, probe.maxPacket, NULL, probe.report, DIRECTION_IN,
	                      USB_TRANSFER_TYPE_INTERRUPT, 0, NULL, reportReceived, NULL) == 0)
		probe.time = millis16();
}

const usbDeviceInstaller_t USB_device_Installers[] = {hubInstaller, probeInstaller, 0};

// ------------------------------------------------------------------ main

static void drawLabels(void)
{
	print("uSVC USB HUB TEST", 0, 0);
	print("ON THE CONSOLE'S PORT", 0, 2);
	print("State      Restarts", 1, 3);
	print("ID     :     Class", 1, 4);
	print("Name", 1, 5);
	print("Hub", 1, 6);
	print("HUB PORTS    Hub status", 0, 7);
	print("Polls       Got    St    R", 1, 12);
	print("DEVICE", 0, 13);
	print("ID     :     Class    Port   Addr", 1, 14);
	print("Name", 1, 15);
	print("If   /  /   Ep    Max    Int", 1, 16);
	print("Step    Reports       Len", 1, 17);
	print("NOTE", 0, 21);
}

static void drawStatus(void)
{
	usbModuleData_t *pUSB = getUSBData();
	printHex(pUSB->usbTaskState, 2, 7, 3);
	printDecimal(restarts, 21, 3);
	if (rootSeen)
	{
		printHex(rootVendor, 4, 4, 4);
		printHex(rootProduct, 4, 9, 4);
		printHex(rootClass, 2, 20, 4);
		print(rootName, 6, 5);
	}
	for (uint8_t i = 0; i < sizeof(hubDescriptor); i++)
		printHex(hubDescriptor[i], 2, 5 + 3 * i, 6);
	printHex(hubConfiguration, 2, 33, 6);
	for (uint8_t i = 0; i < sizeof(hubStatus); i++)
		printHex(hubStatus[i], 2, 24 + 3 * i, 7);
	printDecimal(statusPolls, 7, 12);
	printHex(lastBytesReceived, 2, 17, 12);
	printHex(hub.state, 2, 23, 12);
	for (uint8_t i = 0; i < sizeof(statusReply); i++)
		printHex(statusReply[i], 2, 28 + 3 * i, 12);
	for (uint8_t port = 1; port <= hub.ports; port++)
	{
		uint8_t row = 7 + port;
		uint16_t status = hub.portStatus[port];
		clearRow(row);
		print("P  St      Ch", 1, row);
		printHex(port, 1, 2, row);
		printHex(status, 4, 7, row);
		printHex(hub.portChange[port], 4, 15, row);
		if (status & STATUS_POWERED)
			print("PWR", 20, row);
		if (status & STATUS_CONNECTED)
			print("CONN", 24, row);
		if (status & STATUS_ENABLED)
			print("EN", 29, row);
		if (status & STATUS_LOW_SPEED)
			print("LOW", 32, row);
	}
	if (probe.seen)
	{
		printHex(probe.idVendor, 4, 4, 14);
		printHex(probe.idProduct, 4, 9, 14);
		printHex(probe.deviceClass, 2, 20, 14);
		printHex(probe.hubPort, 1, 28, 14);
		printHex(probe.address, 1, 35, 14);
		memset(&vram[15 * VRAMX + 6], 0, 34);
		print(deviceName, 6, 15);
		printHex(probe.interfaceClass, 2, 4, 16);
		printHex(probe.interfaceSubClass, 2, 7, 16);
		printHex(probe.interfaceProtocol, 2, 10, 16);
		printHex(probe.endpoint, 2, 16, 16);
		printHex(probe.maxPacket, 2, 23, 16);
		printHex(probe.interval, 2, 30, 16);
		printDecimal(probe.reports, 17, 17);
		printHex(probe.lastLength, 2, 27, 17);
		for (uint8_t i = 0; i < 12; i++)
			printHex(probe.report[i], 2, 1 + 3 * i, 18);
	}
	printHex(probe.enumerationStep, 2, 6, 17);
	clearRow(22);
	print(note, 1, 22);
	clearRow(24);
	if (probe.reports && probe.hubPort)
		print("RESULT: DEVICE WORKS THROUGH THE HUB", 0, 24);
	else if (probe.reports)
		print("RESULT: DEVICE WORKS, PLUGGED DIRECTLY", 0, 24);
	else if (probe.ready)
		print("DEVICE SET UP. PRESS ITS BUTTONS.", 0, 24);
	else if (hub.ready)
		print("HUB SET UP. WAITING FOR A DEVICE.", 0, 24);
	else
		print("WAITING FOR A USB DEVICE.", 0, 24);
}

int main(void)
{
	uint16_t frame = 0;
	uint8_t wasAttached = 0;
	memcpy(ramVectorTable, (void *)&_sfixed, sizeof(ramVectorTable));
	SCB->VTOR = ((uint32_t)ramVectorTable & SCB_VTOR_TBLOFF_Msk);
	initUsvc(NULL);
	for (int c = ' '; c < 128 + 6; c++)
		putCharInTile(NULL, c, 0xFF, 0, 0, (uint8_t *)tiles[c - ' '], 0);
	for (int i = 0; i < 200; i++)
		rowRemapTable[i] = i;
	memset(vram, 0, sizeof(vram));
	usbSetProductNameFoundCallBack(productNameFound);
	drawLabels();
	while (1)
	{
		waitForVerticalBlank();
		drawStatus();
		setLed(frame++ & 32);
		do
		{
			usbModuleData_t *pUSB = getUSBData();
			usbHostTask();
			uint8_t attached = (pUSB->usbTaskState & USB_STATE_MASK) != USB_STATE_DETACHED;
			if (wasAttached && !attached)
			{
				// Unplugged, or the stack started over after an error.
				restarts++;
				hub.ready = 0;
				probe.ready = 0;
				probe.bound = 0;
				probe.pipe = 0;
				memset(hub.handled, 0, sizeof(hub.handled));
			}
			wasAttached = attached;
			if (pUSB->usbTaskState == USB_STATE_RUNNING && !rootSeen)
			{
				rootSeen = 1;
				rootVendor = pUSB->pUsbDevices[0].idVendor;
				rootProduct = pUSB->pUsbDevices[0].idProduct;
				rootClass = pUSB->pUsbDevices[0].bDeviceClass;
			}
			hubTask();
			probePoll();
		} while (getCurrentScanLineNumber() < 523);
	}
}
