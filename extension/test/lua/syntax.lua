--[[
  A small but representative Warcraft III Lua script: tables, closures,
  methods, every loop kind, varargs and string building.
]]
local MAX_COUNTERS <const> = 8
local counters = {}

---@class Counter
local Counter = { created = 0 }
Counter.__index = Counter

function Counter.new(start)
    local self = setmetatable({ value = start or 0 }, Counter)
    Counter.created = Counter.created + 1
    return self
end

function Counter:bump(by)
    self.value = self.value + (by or 1)
    return self.value
end

local function isEven(n)
    return n % 2 == 0
end

local function sum(...)
    local total = 0
    for _, v in ipairs({ ... }) do
        total = total + v
    end
    return total
end

function InitCounters()
    for i = 1, MAX_COUNTERS do
        counters[i] = Counter.new(i * 2)
    end
    local i = 0
    while true do
        i = i + 1
        if i > MAX_COUNTERS then
            break
        elseif isEven(i) then
            counters[i]:bump(-1)
        end
    end
    repeat
        i = i - 1
    until i <= 0
    print(("total: %d"):format(sum(1, 2, 3)) .. " of " .. #counters)
    TimerStart(CreateTimer(), 1.0, true, function()
        DestroyTimer(GetExpiredTimer())
    end)
end
