global _syscall_entry 
extern syscall_dispatch

_syscall_entry: 
    swapgs 

    mov [gs:0x08], rsp  ; save user rsp 
    mov rsp, [gs:0x10]  ; load kernel rsp

    push qword [gs:0x08] ; push user rsp on the kernel stack 
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15 
    
    mov rdi, rsp 
    
    call syscall_dispatch 

    mov rsp, rdi

    pop r15 
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    pop rsp 

    swapgs

    sysretq
