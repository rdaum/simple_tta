import string

UNIT_NONE = 0
UNIT_STACK_PUSH_POP = 1
UNIT_STACK_INDEX = 2
UNIT_REGISTER = 3
UNIT_ALU_LEFT = 4
UNIT_ALU_RIGHT = 5
UNIT_ALU_OPERATOR = 6
UNIT_ALU_RESULT = 7
UNIT_MEMORY_IMMEDIATE = 8
UNIT_MEMORY_OPERAND = 9
UNIT_PC = 10
UNIT_ABS_IMMEDIATE = 11
UNIT_ABS_OPERAND = 12

class Instr():

    def __init__(self, sunit, si, dunit, di, soperand=None, doperand=None):
        op = 0
        op |= sunit
        op |= si << 4
        op |= dunit << 16
        op |= di << 20
        self.si = si
        self.di = di
        self.dunit = dunit
        self.sunit = sunit
        self.soperand = soperand
        self.doperand = doperand
        self.op = op

    def hex(self):
        asm = "{:08x}".format(self.op)
        if self.sunit == UNIT_MEMORY_OPERAND or self.sunit == UNIT_ABS_OPERAND:
            asm = asm + " {:08x}".format(self.soperand)
        if self.dunit == UNIT_MEMORY_OPERAND or self.dunit == UNIT_ABS_OPERAND:
            asm = asm + " {:08x}".format(self.doperand)
        return asm

    def asm(self):
        asm = ""
        if self.dunit == UNIT_NONE:
            return "NOP"
        elif self.dunit == UNIT_STACK_PUSH_POP:
            asm = "PUSH "
        elif self.dunit == UNIT_STACK_INDEX:
            asm = "S{:06x} := ".format(self.di)
        elif self.dunit == UNIT_REGISTER:
            asm = "R{:02x} := ".format(self.di)
        elif self.dunit == UNIT_ALU_RIGHT:
            asm = "ALU:RIGHT := "
        elif self.dunit == UNIT_ALU_LEFT:
            asm = "ALU:LEFT := "
        elif self.dunit == UNIT_ALU_OPERATOR:
            asm = "ALU:OPERATOR := "
        elif self.dunit == UNIT_MEMORY_IMMEDIATE:
            asm = "*({:06x}) := ".format(self.di)
        elif self.dunit == UNIT_MEMORY_OPERAND:
            asm = "*({:08x}) := ".format(self.doperand)
        elif self.dunit == UNIT_PC:
            asm = "JMP "

        if self.sunit == UNIT_NONE:
            asm += "#0"
        elif self.sunit == UNIT_STACK_PUSH_POP:
            asm += "POP "
        elif self.sunit == UNIT_STACK_INDEX:
            asm += "S{:06x}".format(self.si)
        elif self.sunit == UNIT_REGISTER:
            asm += "R{:02x}".format(self.si)
        elif self.sunit == UNIT_ALU_RIGHT:
            asm += "ALU:RIGHT "
        elif self.sunit == UNIT_ALU_LEFT:
            asm += "ALU:LEFT"
        elif self.sunit == UNIT_ALU_OPERATOR:
            asm += "ALU:OPERATOR"
        elif self.sunit == UNIT_ALU_RESULT:
            asm += "ALU:RESULT"
        elif self.sunit == UNIT_MEMORY_IMMEDIATE:
            asm += "*({:06x})".format(self.si)
        elif self.sunit == UNIT_MEMORY_OPERAND:
            asm += "*({:08x})".format(self.soperand)
        elif self.sunit == UNIT_PC:
            asm += "PC"
        elif self.sunit == UNIT_ABS_IMMEDIATE:
            asm += "#{:06x}".format(self.si)
        elif self.sunit == UNIT_ABS_OPERAND:
            asm += "#{:08x}".format(self.soperand)

        return asm

def chunks(lst, n):
    """Yield successive n-sized chunks from lst."""
    for i in range(0, len(lst), n):
        yield lst[i:i + n]

def main():
    prgm = [
        Instr(UNIT_ABS_IMMEDIATE, 0x666, UNIT_REGISTER, 0),
        Instr(UNIT_REGISTER, 0, UNIT_ALU_LEFT, 0),
        Instr(UNIT_ABS_IMMEDIATE, 0x123, UNIT_ALU_RIGHT, 0),
        Instr(UNIT_ABS_IMMEDIATE, 0x01, UNIT_ALU_OPERATOR, 0),
        Instr(UNIT_ALU_RESULT, 0, UNIT_REGISTER, 1),
        Instr(UNIT_REGISTER, 1, UNIT_MEMORY_OPERAND, 0, doperand=0x543),
        Instr(UNIT_MEMORY_IMMEDIATE, 0x543, UNIT_MEMORY_IMMEDIATE, 0x666)]

    mem = open("bootmem.mem", "w")
    chks = chunks(prgm, 4)
    for x in chks:
        mem.write(string.join([i.hex() for i in x]))
        mem.write("\n")
    mem.close()

    print(string.join([i.asm() for i in prgm], "\n"))

if __name__ == "__main__":
    main()

