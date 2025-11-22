"use client"

interface StepIndicatorProps {
  stepNumber: number
  currentStep: number
  label: string
  isDevMode: boolean
  onStepClick?: () => void
}

export function StepIndicator({ stepNumber, currentStep, label, isDevMode, onStepClick }: StepIndicatorProps) {
  const isCurrent = currentStep === stepNumber
  const isCompleted = currentStep > stepNumber
  const stateTone = isCurrent || isCompleted ? "text-[#f3b724]" : "text-white/40"
  const numberClasses = [
    "flex h-8 w-8 items-center justify-center rounded-full border text-xs font-medium transition-all duration-300 leading-none pl-0.75",
    isCurrent ? "border-[#f3b724] text-[#f3b724] bg-[#f3b724]/10 shadow-[0_0_12px_rgba(243,183,36,0.3)]" : 
    isCompleted ? "border-[#f3b724]/30 text-[#f3b724] bg-[#f3b724]/5" : 
    "border-white/10 text-white/30",
  ].join(" ")
  
  const marker = (
    <>
      <span className={numberClasses}>
        {isCompleted ? (
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
          </svg>
        ) : stepNumber}
      </span>
      <span className={`${stateTone} text-xs transition-colors duration-300`}>{label}</span>
    </>
  )

  if (isDevMode && onStepClick) {
    return (
      <button
        type="button"
        onClick={onStepClick}
        className="flex flex-col items-center gap-2 text-center transition hover:opacity-80 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white"
        aria-current={isCurrent ? "step" : undefined}
      >
        {marker}
      </button>
    )
  }

  return (
    <div className="flex flex-col items-center gap-2 text-center">
      {marker}
    </div>
  )
}

