global switch_threads 

section .text

; RDI = old_stack_ptr (*mut usize)
; RSI = new_stack_ptr (usize)
; RDX = old_extended_context (*mut ExtendedContext)
; RCX = new_extended_context (*const ExtendedContext)

switch_threads:
    ; step 1: save callee saved regs on old thread's stack
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15

    ; step 2: save extended context
    fxsave64 [rdx]              ; writing into old_extended_context

    ; step 3: swap stack ptrs
    mov [rdi], rsp              ; writing into old_stack_ptr
    mov rsp, rsi                ; read from new_stack_ptr
    
    ; step 4: restore extended context 
    fxrstor64 [rcx]             ; read from new_extended_context

    ; step 5: restore callee saved regs from new thread's stack
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    
    ; ret pops the RIP off the new thread's stack
    ret

global thread_entry_stub
extern unlock_scheduler 

section .text 

thread_entry_stub:
    call unlock_scheduler

    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15

    add rsp, 16 ; skip interrupt number and error code

    iretq
