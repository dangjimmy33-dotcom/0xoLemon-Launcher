#include "gamepad/gamepad.h"

#include <iostream>

#ifdef GAMEPAD_TESTING
extern "C" unsigned int GamepadTestDetectCount(void);
extern "C" void GamepadTestSignalDeviceChange(void);
#endif

int main()
{
#ifndef GAMEPAD_TESTING
    std::cerr << "GAMEPAD_TESTING must be enabled for this test" << std::endl;
    return 1;
#else
    GamepadInit();

    if (GamepadTestDetectCount() != 1) {
        std::cerr << "expected exactly one initial device scan" << std::endl;
        return 1;
    }

    for (int i = 0; i < 10; ++i) {
        GamepadUpdate();
    }

    if (GamepadTestDetectCount() != 1) {
        std::cerr << "unexpected rescans without hotplug" << std::endl;
        return 1;
    }

    GamepadTestSignalDeviceChange();
    GamepadUpdate();

    if (GamepadTestDetectCount() != 2) {
        std::cerr << "expected a rescan after a hotplug signal" << std::endl;
        return 1;
    }

    GamepadShutdown();
    return 0;
#endif
}
